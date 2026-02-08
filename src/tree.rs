use dashmap::{DashMap, DashSet};
use rayon::prelude::*;
use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

use std::sync::OnceLock;

pub fn cpu_count() -> usize {
    static CPU_COUNT: OnceLock<usize> = OnceLock::new();
    *CPU_COUNT.get_or_init(|| {
        std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(4)
    })
}

fn scan_parallel_threshold() -> usize {
    let cpus = cpu_count();

    // 核心数多时更早启用并行: 4核=3, 8核=2, 16核+=2
    if cpus >= 8 {
        2
    } else {
        3
    }
}

#[derive(Debug)]
pub struct DirectoryTree {
    pub dirs: Vec<PathBuf>,
    pub children: HashMap<PathBuf, Vec<PathBuf>>,
    pub leaves: Vec<PathBuf>,
    pub file_count: usize,
    pub total_bytes: u64,
    /// Files in each directory - collected during scan to avoid re-enumeration during deletion
    pub dir_files: HashMap<PathBuf, Vec<PathBuf>>,
}

impl DirectoryTree {
    pub fn new() -> Self {
        Self {
            dirs: Vec::new(),
            children: HashMap::new(),
            leaves: Vec::new(),
            file_count: 0,
            total_bytes: 0,
            dir_files: HashMap::new(),
        }
    }
}

impl Default for DirectoryTree {
    fn default() -> Self {
        Self::new()
    }
}

pub fn discover_tree(root: &Path) -> io::Result<DirectoryTree> {
    let all_dirs: DashSet<PathBuf> = DashSet::new();
    let children_map: DashMap<PathBuf, Vec<PathBuf>> = DashMap::new();
    let dir_files_map: DashMap<PathBuf, Vec<PathBuf>> = DashMap::new();
    let file_count = AtomicUsize::new(0);
    let total_bytes = AtomicU64::new(0);

    scan_parallel(
        root,
        &all_dirs,
        &children_map,
        &dir_files_map,
        &file_count,
        &total_bytes,
    );

    let mut tree = DirectoryTree::new();

    tree.dirs = all_dirs.into_iter().collect();

    tree.children = children_map.into_iter().collect();
    tree.dir_files = dir_files_map.into_iter().collect();

    for dir in &tree.dirs {
        if !tree.children.contains_key(dir) {
            tree.leaves.push(dir.clone());
        }
    }

    tree.file_count = file_count.load(Ordering::Relaxed);
    tree.total_bytes = total_bytes.load(Ordering::Relaxed);

    Ok(tree)
}

fn scan_parallel(
    dir: &Path,
    all_dirs: &DashSet<PathBuf>,
    children_map: &DashMap<PathBuf, Vec<PathBuf>>,
    dir_files_map: &DashMap<PathBuf, Vec<PathBuf>>,
    file_count: &AtomicUsize,
    total_bytes: &AtomicU64,
) {
    all_dirs.insert(dir.to_path_buf());

    let mut child_dirs = Vec::with_capacity(16);
    let mut files = Vec::with_capacity(64);
    let mut local_bytes = 0u64;

    let mut symlink_dirs = Vec::new();

    if let Err(e) = crate::winapi::enumerate_files(dir, |entry| {
        if entry.is_symlink {
            if entry.is_dir {
                symlink_dirs.push(entry.path);
            } else {
                files.push(entry.path);
            }
        } else if entry.is_dir {
            child_dirs.push(entry.path);
        } else {
            files.push(entry.path);
            local_bytes += entry.size;
        }
        Ok(())
    }) {
        eprintln!(
            "Warning: Skipping directory due to enumeration error {}: {}",
            dir.display(),
            e
        );
        return;
    }

    // Register symlink directories as leaf directories (no recursion into them)
    for symlink_dir in &symlink_dirs {
        all_dirs.insert(symlink_dir.clone());
    }

    let local_file_count = files.len();
    if !files.is_empty() {
        dir_files_map.insert(dir.to_path_buf(), files);
        file_count.fetch_add(local_file_count, Ordering::Relaxed);
    }

    if local_bytes > 0 {
        total_bytes.fetch_add(local_bytes, Ordering::Relaxed);
    }

    // Include symlink dirs in children so parent waits for them before removal
    let all_children: Vec<PathBuf> = child_dirs
        .iter()
        .chain(symlink_dirs.iter())
        .cloned()
        .collect();

    if !all_children.is_empty() {
        children_map.insert(dir.to_path_buf(), all_children);
    }

    // Only recurse into non-symlink child directories
    if !child_dirs.is_empty() {
        if child_dirs.len() >= scan_parallel_threshold() {
            child_dirs.par_iter().for_each(|child| {
                scan_parallel(
                    child,
                    all_dirs,
                    children_map,
                    dir_files_map,
                    file_count,
                    total_bytes,
                );
            });
        } else {
            for child in &child_dirs {
                scan_parallel(
                    child,
                    all_dirs,
                    children_map,
                    dir_files_map,
                    file_count,
                    total_bytes,
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_parallel_discover_tree() {
        let temp = std::env::temp_dir().join("rmx_parallel_test");
        let _ = fs::remove_dir_all(&temp);
        fs::create_dir_all(&temp).unwrap();

        fs::create_dir_all(temp.join("a/b/c")).unwrap();
        fs::create_dir_all(temp.join("a/d")).unwrap();
        fs::write(temp.join("a/file1.txt"), "test").unwrap();
        fs::write(temp.join("a/b/file2.txt"), "test").unwrap();
        fs::write(temp.join("a/b/c/file3.txt"), "test").unwrap();

        let tree = discover_tree(&temp).unwrap();

        assert!(tree.dirs.len() >= 4);
        assert_eq!(tree.file_count, 3);
        assert!(!tree.leaves.is_empty());

        let _ = fs::remove_dir_all(&temp);
    }
}
