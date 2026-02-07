use crate::tree::DirectoryTree;
use crossbeam_channel::{unbounded, Receiver, Sender};
use dashmap::DashMap;
use parking_lot::Mutex;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

pub struct Broker {
    child_counts: DashMap<PathBuf, usize>,
    parent_map: DashMap<PathBuf, PathBuf>,
    dir_files: DashMap<PathBuf, Vec<PathBuf>>,
    work_tx: Mutex<Option<Sender<PathBuf>>>,
    total_dirs: usize,
    completed: AtomicUsize,
    done: AtomicBool,
}

impl Broker {
    pub fn new(tree: DirectoryTree) -> (Self, Sender<PathBuf>, Receiver<PathBuf>) {
        let (tx, rx) = unbounded();

        let child_counts = DashMap::new();
        let parent_map = DashMap::new();
        let dir_files = DashMap::new();
        let total_dirs = tree.dirs.len();

        for (parent, children) in tree.children {
            let child_count = children.len();
            for child in children {
                parent_map.insert(child, parent.clone());
            }
            child_counts.insert(parent, child_count);
        }

        for (dir, files) in tree.dir_files {
            dir_files.insert(dir, files);
        }

        let broker = Self {
            child_counts,
            parent_map,
            dir_files,
            work_tx: Mutex::new(Some(tx.clone())),
            total_dirs,
            completed: AtomicUsize::new(0),
            done: AtomicBool::new(false),
        };

        for leaf in tree.leaves {
            tx.send(leaf).ok();
        }

        (broker, tx, rx)
    }

    pub fn take_files(&self, dir: &PathBuf) -> Option<Vec<PathBuf>> {
        self.dir_files.remove(dir).map(|(_, files)| files)
    }

    pub fn new_dirs_only(tree: DirectoryTree) -> (Self, Sender<PathBuf>, Receiver<PathBuf>) {
        let (tx, rx) = unbounded();

        let child_counts = DashMap::new();
        let parent_map = DashMap::new();
        let total_dirs = tree.dirs.len();

        for (parent, children) in tree.children {
            let child_count = children.len();
            for child in children {
                parent_map.insert(child, parent.clone());
            }
            child_counts.insert(parent, child_count);
        }

        let broker = Self {
            child_counts,
            parent_map,
            dir_files: DashMap::new(),
            work_tx: Mutex::new(Some(tx.clone())),
            total_dirs,
            completed: AtomicUsize::new(0),
            done: AtomicBool::new(false),
        };

        for leaf in tree.leaves {
            tx.send(leaf).ok();
        }

        (broker, tx, rx)
    }

    pub fn mark_complete(&self, dir: PathBuf) {
        let completed = self.completed.fetch_add(1, Ordering::Relaxed) + 1;

        if completed == self.total_dirs {
            self.done.store(true, Ordering::Release);
            *self.work_tx.lock() = None;
            return;
        }

        // Fast path: skip if already done
        if self.done.load(Ordering::Acquire) {
            return;
        }

        let parent = self.parent_map.get(&dir).map(|r| r.clone());

        if let Some(parent_path) = parent {
            let should_send = {
                let mut entry = match self.child_counts.get_mut(&parent_path) {
                    Some(e) => e,
                    None => return,
                };
                *entry -= 1;
                *entry == 0
            };

            if should_send {
                self.child_counts.remove(&parent_path);
                // Only acquire lock when we actually need to send
                if let Some(ref tx) = *self.work_tx.lock() {
                    tx.send(parent_path).ok();
                }
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
