use crate::broker::{Broker, WorkItem};
use crate::error::FailedItem;
use crate::winapi::{
    delete_file, force_close_file_handles, is_file_in_use_error, is_not_found_error,
    kill_locking_processes, kill_locking_processes_batch, remove_dir,
};
use crossbeam_channel::Receiver;
use crossbeam_queue::SegQueue;
use rayon::prelude::*;
use std::path::PathBuf;
use std::sync::Arc;
use std::thread::{self, JoinHandle};

#[derive(Clone)]
pub struct WorkerConfig {
    pub verbose: bool,
    pub ignore_errors: bool,
    pub kill_processes: bool,
}

impl Default for WorkerConfig {
    fn default() -> Self {
        Self {
            verbose: false,
            ignore_errors: true,
            kill_processes: false,
        }
    }
}

pub struct ErrorTracker {
    failures: SegQueue<FailedItem>,
}

impl ErrorTracker {
    pub fn new() -> Self {
        Self {
            failures: SegQueue::new(),
        }
    }

    pub fn record_failure(&self, item: FailedItem) {
        self.failures.push(item);
    }

    pub fn get_failures(&self) -> Vec<FailedItem> {
        let mut result = Vec::new();
        while let Some(item) = self.failures.pop() {
            result.push(item);
        }
        result
    }
}

impl Default for ErrorTracker {
    fn default() -> Self {
        Self::new()
    }
}

pub fn spawn_workers(
    count: usize,
    rx: Receiver<WorkItem>,
    broker: Arc<Broker>,
    config: WorkerConfig,
    error_tracker: Arc<ErrorTracker>,
) -> Vec<JoinHandle<()>> {
    (0..count)
        .map(|i| {
            let rx = rx.clone();
            let broker = broker.clone();
            let config = config.clone();
            let error_tracker = error_tracker.clone();
            thread::Builder::new()
                .name(format!("worker-{}", i))
                .spawn(move || worker_thread(rx, broker, config, error_tracker))
                .expect("Failed to spawn worker thread")
        })
        .collect()
}

fn worker_thread(
    rx: Receiver<WorkItem>,
    broker: Arc<Broker>,
    config: WorkerConfig,
    error_tracker: Arc<ErrorTracker>,
) {
    while let Ok(item) = rx.recv() {
        match item {
            WorkItem::DeleteFiles { files, parent_dir } => {
                delete_files_from_list(&files, &config, &error_tracker);
                broker.mark_batch_complete(&parent_dir);
            }
            WorkItem::ProcessDir(dir) => {
                process_directory(&dir, &broker, &config, &error_tracker);
            }
            WorkItem::Shutdown => break,
        }
    }
}

fn process_directory(
    dir: &PathBuf,
    broker: &Arc<Broker>,
    config: &WorkerConfig,
    error_tracker: &Arc<ErrorTracker>,
) {
    if let Some(files) = broker.take_files(dir) {
        delete_files_from_list(&files, config, error_tracker);
    }

    if let Err(e) = remove_dir(dir) {
        if is_not_found_error(&e) {
            broker.mark_complete(dir.clone());
            return;
        }

        if config.kill_processes && is_file_in_use_error(&e) {
            let _ = kill_locking_processes(dir, config.verbose);
            if let Ok(()) = remove_dir(dir) {
                broker.mark_complete(dir.clone());
                return;
            }

            let _ = force_close_file_handles(std::slice::from_ref(dir), config.verbose);
            match remove_dir(dir) {
                Ok(()) => {
                    broker.mark_complete(dir.clone());
                    return;
                }
                Err(retry_err) if is_not_found_error(&retry_err) => {
                    broker.mark_complete(dir.clone());
                    return;
                }
                _ => {}
            }
        }

        let msg = e.to_string();
        if config.verbose {
            eprintln!("Warning: Failed to remove {}: {}", dir.display(), msg);
        }
        error_tracker.record_failure(FailedItem {
            path: dir.clone(),
            error: msg,
            is_dir: true,
        });

        broker.mark_complete(dir.clone());
        return;
    }

    broker.mark_complete(dir.clone());
}

fn cpu_count() -> usize {
    static CPU_COUNT: std::sync::OnceLock<usize> = std::sync::OnceLock::new();
    *CPU_COUNT.get_or_init(|| {
        std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(4)
    })
}

fn parallel_threshold() -> usize {
    let cpus = cpu_count();

    match cpus {
        1..=4 => 24,
        5..=8 => 16,
        9..=16 => 12,
        _ => 8,
    }
}

fn min_chunk_size() -> usize {
    let cpus = cpu_count();
    (cpus * 2).clamp(4, 16)
}

fn delete_files_from_list(
    files: &[PathBuf],
    config: &WorkerConfig,
    error_tracker: &Arc<ErrorTracker>,
) {
    if files.is_empty() {
        return;
    }

    if files.len() < parallel_threshold() {
        delete_files_sequential(files, config, error_tracker);
    } else {
        delete_files_parallel(files, config, error_tracker);
    }
}

fn delete_files_sequential(
    files: &[PathBuf],
    config: &WorkerConfig,
    error_tracker: &Arc<ErrorTracker>,
) {
    let mut locked_files = Vec::new();

    for path in files {
        if let Err(e) = delete_file(path) {
            if is_not_found_error(&e) {
                continue;
            }
            if config.kill_processes && is_file_in_use_error(&e) {
                locked_files.push((path.clone(), e));
            } else {
                record_file_error(path, &e, config, error_tracker);
            }
        }
    }

    handle_locked_files(locked_files, config, error_tracker);
}

fn delete_files_parallel(
    files: &[PathBuf],
    config: &WorkerConfig,
    error_tracker: &Arc<ErrorTracker>,
) {
    let locked_files: Vec<(PathBuf, std::io::Error)> = files
        .par_iter()
        .with_min_len(min_chunk_size())
        .filter_map(|path| match delete_file(path) {
            Ok(()) => None,
            Err(e) if is_not_found_error(&e) => None,
            Err(e) => {
                if config.kill_processes && is_file_in_use_error(&e) {
                    Some((path.clone(), e))
                } else {
                    record_file_error(path, &e, config, error_tracker);
                    None
                }
            }
        })
        .collect();

    handle_locked_files(locked_files, config, error_tracker);
}

#[inline]
fn record_file_error(
    path: &std::path::Path,
    error: &std::io::Error,
    config: &WorkerConfig,
    error_tracker: &Arc<ErrorTracker>,
) {
    let msg = error.to_string();
    if config.verbose {
        eprintln!("Warning: Failed to delete {}: {}", path.display(), msg);
    }
    error_tracker.record_failure(FailedItem {
        path: path.to_path_buf(),
        error: msg,
        is_dir: false,
    });
}

fn handle_locked_files(
    locked_files: Vec<(PathBuf, std::io::Error)>,
    config: &WorkerConfig,
    error_tracker: &Arc<ErrorTracker>,
) {
    if locked_files.is_empty() {
        return;
    }

    let mut paths: Vec<PathBuf> = locked_files.into_iter().map(|(p, _)| p).collect();

    let _ = kill_locking_processes_batch(&paths, config.verbose);

    paths.retain(|path| match delete_file(path) {
        Ok(()) => false,
        Err(e) if is_not_found_error(&e) => false,
        Err(e) if is_file_in_use_error(&e) => true,
        Err(e) => {
            record_file_error(path, &e, config, error_tracker);
            false
        }
    });

    if paths.is_empty() {
        return;
    }

    let _ = force_close_file_handles(&paths, config.verbose);

    for path in &paths {
        if let Err(e) = delete_file(path) {
            if !is_not_found_error(&e) {
                record_file_error(path, &e, config, error_tracker);
            }
        }
    }
}
