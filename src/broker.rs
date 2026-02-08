use crate::tree::DirectoryTree;
use crossbeam_channel::{unbounded, Receiver, Sender};
use dashmap::DashMap;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

/// Threshold: directories with more files than this get split into batches
const BATCH_THRESHOLD: usize = 1024;
/// Number of files per batch when splitting large directories
const BATCH_SIZE: usize = 256;

/// Work item dispatched through the broker channel.
pub enum WorkItem {
    /// A directory ready for processing: delete its remaining files, remove the
    /// (now-empty) directory, then call `mark_complete`.
    ProcessDir(PathBuf),
    /// A batch of files to delete. Once done, call `mark_batch_complete` with
    /// the parent directory. When all batches for a directory finish, a
    /// `ProcessDir` is automatically enqueued.
    DeleteFiles {
        files: Vec<PathBuf>,
        parent_dir: PathBuf,
    },
    Shutdown,
}

pub struct Broker {
    /// Remaining child-directory count per parent. Uses AtomicUsize inside
    /// DashMap so decrement only needs a read-lock (fetch_sub) not a write-lock.
    child_counts: DashMap<PathBuf, AtomicUsize>,
    /// Parent lookup — populated once during construction, never mutated.
    /// Plain HashMap avoids DashMap overhead for read-only data.
    parent_map: HashMap<PathBuf, PathBuf>,
    dir_files: DashMap<PathBuf, Vec<PathBuf>>,
    /// Tracks in-flight file batches per directory.
    pending_batches: DashMap<PathBuf, AtomicUsize>,
    /// Direct sender — no Mutex wrapper. crossbeam Sender is already thread-safe.
    work_tx: Sender<WorkItem>,
    total_dirs: usize,
    /// Number of worker threads, used to send Shutdown sentinels.
    worker_count: usize,
    completed: AtomicUsize,
    done: AtomicBool,
}

impl Broker {
    pub fn new(tree: DirectoryTree, worker_count: usize) -> (Self, Receiver<WorkItem>) {
        let (tx, rx) = unbounded();

        let child_counts = DashMap::new();
        let mut parent_map = HashMap::new();
        let dir_files = DashMap::new();
        let total_dirs = tree.dirs.len();

        for (parent, children) in tree.children {
            let child_count = children.len();
            for child in children {
                parent_map.insert(child, parent.clone());
            }
            child_counts.insert(parent, AtomicUsize::new(child_count));
        }

        for (dir, files) in tree.dir_files {
            dir_files.insert(dir, files);
        }

        let broker = Self {
            child_counts,
            parent_map,
            dir_files,
            pending_batches: DashMap::new(),
            work_tx: tx,
            total_dirs,
            worker_count,
            completed: AtomicUsize::new(0),
            done: AtomicBool::new(false),
        };

        // Schedule initial leaf directories (may batch large ones)
        for leaf in tree.leaves {
            broker.schedule_directory(&leaf);
        }

        (broker, rx)
    }

    pub fn take_files(&self, dir: &PathBuf) -> Option<Vec<PathBuf>> {
        self.dir_files.remove(dir).map(|(_, files)| files)
    }

    pub fn new_dirs_only(tree: DirectoryTree, worker_count: usize) -> (Self, Receiver<WorkItem>) {
        let (tx, rx) = unbounded();

        let child_counts = DashMap::new();
        let mut parent_map = HashMap::new();
        let total_dirs = tree.dirs.len();

        for (parent, children) in tree.children {
            let child_count = children.len();
            for child in children {
                parent_map.insert(child, parent.clone());
            }
            child_counts.insert(parent, AtomicUsize::new(child_count));
        }

        let broker = Self {
            child_counts,
            parent_map,
            dir_files: DashMap::new(),
            pending_batches: DashMap::new(),
            work_tx: tx.clone(),
            total_dirs,
            worker_count,
            completed: AtomicUsize::new(0),
            done: AtomicBool::new(false),
        };

        for leaf in tree.leaves {
            tx.send(WorkItem::ProcessDir(leaf)).ok();
        }

        (broker, rx)
    }

    /// Decide how to dispatch a directory that is ready for processing.
    ///
    /// - Small directory (≤ BATCH_THRESHOLD files): send a single `ProcessDir`.
    /// - Large directory (> BATCH_THRESHOLD files): split files into batches,
    ///   send `DeleteFiles` for each chunk, and defer `ProcessDir` until all
    ///   batches complete.
    fn schedule_directory(&self, dir: &PathBuf) {
        let file_count = self.dir_files.get(dir).map(|f| f.len()).unwrap_or(0);

        if file_count > BATCH_THRESHOLD {
            if let Some((_, files)) = self.dir_files.remove(dir) {
                let batch_count = files.len().div_ceil(BATCH_SIZE);
                self.pending_batches
                    .insert(dir.clone(), AtomicUsize::new(batch_count));

                for chunk in files.chunks(BATCH_SIZE) {
                    self.work_tx
                        .send(WorkItem::DeleteFiles {
                            files: chunk.to_vec(),
                            parent_dir: dir.clone(),
                        })
                        .ok();
                }
            }
        } else {
            self.work_tx.send(WorkItem::ProcessDir(dir.clone())).ok();
        }
    }

    /// Called by a worker after finishing a `DeleteFiles` batch.
    /// When all batches for a directory are done, enqueues `ProcessDir` for it.
    pub fn mark_batch_complete(&self, dir: &PathBuf) {
        if let Some(counter) = self.pending_batches.get(dir) {
            let prev = counter.value().fetch_sub(1, Ordering::AcqRel);
            if prev == 1 {
                // Last batch — remove tracker and enqueue directory removal
                drop(counter);
                self.pending_batches.remove(dir);

                self.work_tx.send(WorkItem::ProcessDir(dir.clone())).ok();
            }
        }
    }

    pub fn mark_complete(&self, dir: PathBuf) {
        let completed = self.completed.fetch_add(1, Ordering::Relaxed) + 1;

        if completed == self.total_dirs {
            self.done.store(true, Ordering::Release);
            // Send shutdown sentinels to all workers instead of dropping the sender.
            for _ in 0..self.worker_count {
                self.work_tx.send(WorkItem::Shutdown).ok();
            }
            return;
        }

        // Fast path: skip if already done
        if self.done.load(Ordering::Acquire) {
            return;
        }

        let parent = self.parent_map.get(&dir).cloned();

        if let Some(parent_path) = parent {
            // Read-lock only: fetch_sub on AtomicUsize inside DashMap entry.
            let should_send = if let Some(entry) = self.child_counts.get(&parent_path) {
                entry.value().fetch_sub(1, Ordering::AcqRel) == 1
            } else {
                return;
            };

            if should_send {
                self.child_counts.remove(&parent_path);
                self.schedule_directory(&parent_path);
            }
        }
    }

    pub fn completed_count(&self) -> usize {
        self.completed.load(Ordering::Relaxed)
    }

    pub fn total_dirs(&self) -> usize {
        self.total_dirs
    }
}
