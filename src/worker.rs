use crate::broker::Broker;
use crate::error::FailedItem;
use crate::winapi::{delete_file, enumerate_files, remove_dir};
use crossbeam_channel::Receiver;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};

#[derive(Clone)]
pub struct WorkerConfig {
    pub verbose: bool,
    pub ignore_errors: bool,
}

impl Default for WorkerConfig {
    fn default() -> Self {
        Self {
            verbose: false,
            ignore_errors: true,
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
        if let Err(e) = delete_files_in_dir(&dir, &config, &error_tracker) {
            if config.verbose {
                eprintln!(
                    "Warning: Failed to delete files in {}: {}",
                    dir.display(),
                    e
                );
            }
        }

        if let Err(e) = remove_dir(&dir) {
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

fn delete_files_in_dir(
    dir: &Path,
    config: &WorkerConfig,
    error_tracker: &Arc<ErrorTracker>,
) -> std::io::Result<()> {
    enumerate_files(dir, |path, is_dir| {
        if !is_dir {
            if let Err(e) = delete_file(path) {
                let msg = format!("{}", e);
                error_tracker.record_failure(FailedItem {
                    path: path.to_path_buf(),
                    error: msg.clone(),
                    is_dir: false,
                });

                if config.verbose {
                    eprintln!("Warning: Failed to delete {}: {}", path.display(), msg);
                }
            }
        }
        Ok(())
    })
}
