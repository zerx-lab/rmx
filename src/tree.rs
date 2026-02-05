use std::collections::{HashMap, HashSet};
use std::io;
use std::path::{Path, PathBuf};

#[derive(Debug)]
pub struct DirectoryTree {
    pub dirs: Vec<PathBuf>,
    pub children: HashMap<PathBuf, Vec<PathBuf>>,
    pub leaves: Vec<PathBuf>,
    pub file_count: usize,
}

impl DirectoryTree {
    pub fn new() -> Self {
        Self {
            dirs: Vec::new(),
            children: HashMap::new(),
            leaves: Vec::new(),
            file_count: 0,
        }
    }
}

impl Default for DirectoryTree {
    fn default() -> Self {
        Self::new()
    }
}

pub fn discover_tree(root: &Path) -> io::Result<DirectoryTree> {
    let mut tree = DirectoryTree::new();
    let mut all_dirs = HashSet::new();
    let mut has_children = HashSet::new();
    let mut file_count = 0;

    scan_recursive(
        root,
        &mut all_dirs,
        &mut tree.children,
        &mut has_children,
        &mut file_count,
    )?;

    tree.dirs = all_dirs.iter().cloned().collect();
    tree.dirs.sort();

    for dir in &tree.dirs {
        if !has_children.contains(dir) {
            tree.leaves.push(dir.clone());
        }
    }

    tree.file_count = file_count;

    Ok(tree)
}

fn scan_recursive(
    dir: &Path,
    all_dirs: &mut HashSet<PathBuf>,
    children_map: &mut HashMap<PathBuf, Vec<PathBuf>>,
    has_children: &mut HashSet<PathBuf>,
    file_count: &mut usize,
) -> io::Result<()> {
    all_dirs.insert(dir.to_path_buf());

    let mut child_dirs = Vec::new();

    if let Err(e) = crate::winapi::enumerate_files(dir, |path, is_dir| {
        if is_dir {
            child_dirs.push(path.to_path_buf());
        } else {
            *file_count += 1;
        }
        Ok(())
    }) {
        eprintln!("Warning: Cannot read {}: {}", dir.display(), e);
        return Ok(());
    }

    if !child_dirs.is_empty() {
        has_children.insert(dir.to_path_buf());

        for child in &child_dirs {
            scan_recursive(child, all_dirs, children_map, has_children, file_count)?;
        }

        children_map.insert(dir.to_path_buf(), child_dirs);
    }

    Ok(())
}
