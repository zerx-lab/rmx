use crate::tree::DirectoryTree;
use crossbeam_channel::{unbounded, Receiver, Sender};
use dashmap::DashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

pub struct Broker {
    child_counts: DashMap<PathBuf, usize>,
    parent_map: DashMap<PathBuf, PathBuf>,
    work_tx: Mutex<Option<Sender<PathBuf>>>,
    total_dirs: usize,
    completed: AtomicUsize,
}

impl Broker {
    pub fn new(tree: DirectoryTree) -> (Self, Sender<PathBuf>, Receiver<PathBuf>) {
        let (tx, rx) = unbounded();

        let child_counts = DashMap::new();
        let parent_map = DashMap::new();
        let total_dirs = tree.dirs.len();

        for (parent, children) in &tree.children {
            child_counts.insert(parent.clone(), children.len());
            for child in children {
                parent_map.insert(child.clone(), parent.clone());
            }
        }

        let broker = Self {
            child_counts,
            parent_map,
            work_tx: Mutex::new(Some(tx.clone())),
            total_dirs,
            completed: AtomicUsize::new(0),
        };

        for leaf in tree.leaves {
            tx.send(leaf).ok();
        }

        (broker, tx, rx)
    }

    pub fn mark_complete(&self, dir: PathBuf) {
        let completed = self.completed.fetch_add(1, Ordering::SeqCst) + 1;

        if completed == self.total_dirs {
            *self.work_tx.lock().unwrap() = None;
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
                if let Some(ref tx) = *self.work_tx.lock().unwrap() {
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
