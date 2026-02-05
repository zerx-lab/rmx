use crate::broker::Broker;
use crate::error::FailedItem;
use crate::winapi::{
    delete_file, is_file_in_use_error, kill_locking_processes, kill_locking_processes_batch,
    remove_dir,
};
use crossbeam_channel::Receiver;
use rayon::prelude::*;
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

    let locked_files: Mutex<Vec<(PathBuf, std::io::Error)>> = Mutex::new(Vec::new());

    files.par_iter().for_each(|path| {
        if let Err(e) = delete_file(path) {
            if config.kill_processes && is_file_in_use_error(&e) {
                locked_files.lock().unwrap().push((path.clone(), e));
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
    });

    let locked = locked_files.into_inner().unwrap();
    if !locked.is_empty() {
        let paths: Vec<PathBuf> = locked.iter().map(|(p, _)| p.clone()).collect();
        if let Ok(killed) = kill_locking_processes_batch(&paths, config.verbose) {
            if !killed.is_empty() {
                paths.par_iter().for_each(|path| {
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
                });
            } else {
                for (path, e) in locked {
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
