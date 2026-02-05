use crate::broker::Broker;
use crate::error::FailedItem;
use crate::winapi::{
    delete_file, is_file_in_use_error, kill_locking_processes, kill_locking_processes_batch,
    remove_dir,
};
use crossbeam_channel::Receiver;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
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
    failures: Mutex<Vec<FailedItem>>,
}

impl ErrorTracker {
    pub fn new() -> Self {
        Self {
            failures: Mutex::new(Vec::new()),
        }
    }

    pub fn record_failure(&self, item: FailedItem) {
        self.failures.lock().unwrap().push(item);
    }

    pub fn get_failures(&self) -> Vec<FailedItem> {
        self.failures.lock().unwrap().clone()
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

pub fn spawn_dir_workers(
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
                .name(format!("dir-worker-{}", i))
                .spawn(move || dir_worker_thread(rx, broker, config, error_tracker))
                .expect("Failed to spawn dir worker thread")
        })
        .collect()
}

fn dir_worker_thread(
    rx: Receiver<PathBuf>,
    broker: Arc<Broker>,
    config: WorkerConfig,
    error_tracker: Arc<ErrorTracker>,
) {
    while let Ok(dir) = rx.recv() {
        if let Err(e) = remove_dir(&dir) {
            if config.kill_processes && is_file_in_use_error(&e) {
                if let Ok(killed) = kill_locking_processes(&dir, config.verbose) {
                    if !killed.is_empty() {
                        if let Ok(()) = remove_dir(&dir) {
                            broker.mark_complete(dir);
                            continue;
                        }
                    }
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

            continue;
        }

        broker.mark_complete(dir);
    }
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
            if config.kill_processes && is_file_in_use_error(&e) {
                if let Ok(killed) = kill_locking_processes(&dir, config.verbose) {
                    if !killed.is_empty() {
                        if let Ok(()) = remove_dir(&dir) {
                            broker.mark_complete(dir);
                            continue;
                        }
                    }
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

            continue;
        }

        broker.mark_complete(dir);
    }
}

fn delete_files_from_list(
    files: &[PathBuf],
    config: &WorkerConfig,
    error_tracker: &Arc<ErrorTracker>,
) {
    if files.is_empty() {
        return;
    }

    let mut locked_files: Vec<(PathBuf, std::io::Error)> = Vec::new();

    for path in files {
        if let Err(e) = delete_file(path) {
            if config.kill_processes && is_file_in_use_error(&e) {
                locked_files.push((path.clone(), e));
            } else {
                let msg = format!("{}", e);
                error_tracker.record_failure(FailedItem {
                    path: path.clone(),
                    error: msg.clone(),
                    is_dir: false,
                });
                if config.verbose {
                    eprintln!("Warning: Failed to delete {}: {}", path.display(), msg);
                }
            }
        }
    }

    if !locked_files.is_empty() {
        let paths: Vec<PathBuf> = locked_files.iter().map(|(p, _)| p.clone()).collect();
        if let Ok(killed) = kill_locking_processes_batch(&paths, config.verbose) {
            if !killed.is_empty() {
                for path in &paths {
                    if let Err(e) = delete_file(path) {
                        let msg = format!("{}", e);
                        error_tracker.record_failure(FailedItem {
                            path: path.clone(),
                            error: msg.clone(),
                            is_dir: false,
                        });
                        if config.verbose {
                            eprintln!("Warning: Failed to delete {}: {}", path.display(), msg);
                        }
                    }
                }
            } else {
                for (path, e) in locked_files {
                    let msg = format!("{}", e);
                    error_tracker.record_failure(FailedItem {
                        path,
                        error: msg,
                        is_dir: false,
                    });
                }
            }
        }
    }
}
