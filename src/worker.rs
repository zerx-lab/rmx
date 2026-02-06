use crate::broker::Broker;
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
    rx: Receiver<PathBuf>,
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
    rx: Receiver<PathBuf>,
    broker: Arc<Broker>,
    config: WorkerConfig,
    error_tracker: Arc<ErrorTracker>,
) {
    while let Ok(dir) = rx.recv() {
        if let Some(files) = broker.take_files(&dir) {
            delete_files_from_list(&files, &config, &error_tracker);
        }

        if let Err(e) = remove_dir(&dir) {
            if is_not_found_error(&e) {
                broker.mark_complete(dir);
                continue;
            }

            if config.kill_processes && is_file_in_use_error(&e) {
                let _ = force_close_file_handles(std::slice::from_ref(&dir), config.verbose);
                if let Ok(()) = remove_dir(&dir) {
                    broker.mark_complete(dir);
                    continue;
                }

                let _ = kill_locking_processes(&dir, config.verbose);
                match remove_dir(&dir) {
                    Ok(()) => {
                        broker.mark_complete(dir);
                        continue;
                    }
                    Err(retry_err) if is_not_found_error(&retry_err) => {
                        broker.mark_complete(dir);
                        continue;
                    }
                    _ => {}
                }
            }

            let msg = format!("{}", e);
            error_tracker.record_failure(FailedItem {
                path: dir.clone(),
                error: msg.clone(),
                is_dir: true,
            });

            if config.verbose {
                eprintln!("Warning: Failed to remove {}: {}", dir.display(), msg);
            }

            broker.mark_complete(dir);
            continue;
        }

        broker.mark_complete(dir);
    }
}

fn parallel_threshold() -> usize {
    let cpus = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4);

    // 核心数越多，阈值可以越低（更早并行化）
    // 4核: 24, 8核: 16, 16核: 12, 32核+: 8
    match cpus {
        1..=4 => 24,
        5..=8 => 16,
        9..=16 => 12,
        _ => 8,
    }
}

fn min_chunk_size() -> usize {
    let cpus = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4);

    // 确保每个线程有足够工作量，避免调度开销
    // chunk_size = max(4, 文件数 / (核心数 * 2))
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
    error_tracker.record_failure(FailedItem {
        path: path.to_path_buf(),
        error: msg.clone(),
        is_dir: false,
    });
    if config.verbose {
        eprintln!("Warning: Failed to delete {}: {}", path.display(), msg);
    }
}

fn handle_locked_files(
    locked_files: Vec<(PathBuf, std::io::Error)>,
    config: &WorkerConfig,
    error_tracker: &Arc<ErrorTracker>,
) {
    if locked_files.is_empty() {
        return;
    }

    let paths: Vec<PathBuf> = locked_files.iter().map(|(p, _)| p.clone()).collect();

    let _ = force_close_file_handles(&paths, config.verbose);

    let mut still_locked = Vec::new();
    for path in &paths {
        if let Err(e) = delete_file(path) {
            if is_not_found_error(&e) {
                continue;
            }
            if is_file_in_use_error(&e) {
                still_locked.push(path.clone());
            } else {
                record_file_error(path, &e, config, error_tracker);
            }
        }
    }

    if still_locked.is_empty() {
        return;
    }

    let _ = kill_locking_processes_batch(&still_locked, config.verbose);

    for path in &still_locked {
        if let Err(e) = delete_file(path) {
            if !is_not_found_error(&e) {
                record_file_error(path, &e, config, error_tracker);
            }
        }
    }
}
