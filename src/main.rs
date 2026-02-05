use clap::Parser;
use rmx::{broker::Broker, error::Error, safety, tree, worker};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process;
use std::sync::Arc;
use std::time::Instant;

#[derive(Parser, Debug)]
#[command(name = "rmx")]
#[command(version)]
#[command(about = "Fast parallel file/directory deletion for Windows (rm-compatible)")]
#[command(after_help = "EXAMPLES:\n  \
  rmx file.txt                    Delete file (with confirmation)\n  \
  rmx -f file.txt                 Force delete file (no confirmation)\n  \
  rmx -r ./node_modules           Delete directory (with confirmation)\n  \
  rmx -rf ./target                Force delete directory (no confirmation)\n  \
  rmx -rfv ./dist                 Force delete with verbose output\n  \
  rmx -rf dir1 dir2 dir3          Delete multiple directories")]
struct Args {
    #[arg(required = true)]
    paths: Vec<PathBuf>,

    #[arg(
        short = 'f',
        long = "force",
        help = "Force deletion without confirmation"
    )]
    force: bool,

    #[arg(
        short = 'r',
        short_alias = 'R',
        long = "recursive",
        help = "Remove directories and their contents recursively"
    )]
    recursive: bool,

    #[arg(
        short = 't',
        long,
        help = "Number of worker threads (default: CPU count)"
    )]
    threads: Option<usize>,

    #[arg(
        short = 'n',
        long = "dry-run",
        help = "Dry run - show what would be deleted"
    )]
    dry_run: bool,

    #[arg(short = 'v', long = "verbose", help = "Explain what is being done")]
    verbose: bool,

    #[arg(long = "stats", help = "Show detailed statistics")]
    stats: bool,

    #[arg(long = "no-preserve-root", help = "Do not treat '/' specially")]
    no_preserve_root: bool,
}

fn main() {
    let args = Args::parse();

    if let Err(e) = run(args) {
        eprintln!("rmx: {}", e);
        process::exit(e.exit_code());
    }
}

fn run(args: Args) -> Result<(), Error> {
    let mut total_stats = DeletionStats::default();
    let mut all_failures = Vec::new();
    let mut failed_paths = Vec::new();

    for path in &args.paths {
        match process_path(path, &args) {
            Ok(stats) => total_stats.merge(&stats),
            Err(e) => {
                eprintln!("rmx: cannot remove '{}': {}", path.display(), e);
                failed_paths.push(path.clone());
                if let Error::PartialFailure { errors, .. } = e {
                    all_failures.extend(errors);
                }
            }
        }
    }

    if args.stats {
        print_summary(&total_stats, &args);
    }

    if !failed_paths.is_empty() || !all_failures.is_empty() {
        Err(Error::PartialFailure {
            total: total_stats.total_items(),
            failed: all_failures.len() + failed_paths.len(),
            errors: all_failures,
        })
    } else {
        Ok(())
    }
}

#[derive(Default)]
struct DeletionStats {
    dirs_deleted: usize,
    files_deleted: usize,
    total_time: std::time::Duration,
}

impl DeletionStats {
    fn merge(&mut self, other: &DeletionStats) {
        self.dirs_deleted += other.dirs_deleted;
        self.files_deleted += other.files_deleted;
        self.total_time += other.total_time;
    }

    fn total_items(&self) -> usize {
        self.dirs_deleted + self.files_deleted
    }
}

fn print_summary(stats: &DeletionStats, args: &Args) {
    if args.stats {
        println!("\nStatistics:");
        println!("  Directories: {}", stats.dirs_deleted);
        println!("  Files:       {}", stats.files_deleted);
        println!("  Total:       {}", stats.total_items());
        println!("  Time:        {:.2?}", stats.total_time);
        if stats.total_time.as_secs_f64() > 0.0 {
            let throughput = stats.total_items() as f64 / stats.total_time.as_secs_f64();
            println!("  Throughput:  {:.0} items/sec", throughput);
        }
    }
}

fn process_path(path: &Path, args: &Args) -> Result<DeletionStats, Error> {
    let exists = rmx::winapi::path_exists(path);
    let is_dir = rmx::winapi::is_directory(path);

    if !exists {
        if args.force {
            return try_force_delete_file(path, args);
        }
        return Err(Error::InvalidPath {
            path: path.to_path_buf(),
            reason: "No such file or directory".to_string(),
        });
    }

    if is_dir {
        process_directory(path, args)
    } else {
        process_file(path, args)
    }
}

fn process_file(path: &Path, args: &Args) -> Result<DeletionStats, Error> {
    if args.dry_run {
        if args.verbose {
            println!("would remove '{}'", path.display());
        }
        return Ok(DeletionStats {
            files_deleted: 1,
            ..Default::default()
        });
    }

    if !args.force {
        if !confirm_deletion(path, false)? {
            return Ok(DeletionStats::default());
        }
    }

    let start = Instant::now();

    rmx::winapi::delete_file(path).map_err(|e| Error::io_with_path(path.to_path_buf(), e))?;

    let elapsed = start.elapsed();

    if args.verbose {
        println!("removed '{}'", path.display());
    }

    Ok(DeletionStats {
        files_deleted: 1,
        total_time: elapsed,
        ..Default::default()
    })
}

fn try_force_delete_file(path: &Path, args: &Args) -> Result<DeletionStats, Error> {
    if args.dry_run {
        if args.verbose {
            println!("would remove '{}' (force)", path.display());
        }
        return Ok(DeletionStats {
            files_deleted: 1,
            ..Default::default()
        });
    }

    let start = Instant::now();

    match rmx::winapi::delete_file(path) {
        Ok(()) => {
            let elapsed = start.elapsed();
            if args.verbose {
                println!("removed '{}'", path.display());
            }
            Ok(DeletionStats {
                files_deleted: 1,
                total_time: elapsed,
                ..Default::default()
            })
        }
        Err(e) => {
            if args.force {
                Ok(DeletionStats::default())
            } else {
                Err(Error::io_with_path(path.to_path_buf(), e))
            }
        }
    }
}

fn process_directory(path: &Path, args: &Args) -> Result<DeletionStats, Error> {
    if !args.no_preserve_root {
        match safety::check_path_safety(path) {
            safety::SafetyCheck::Safe => {}
            safety::SafetyCheck::Dangerous {
                reason,
                can_override: false,
            } => {
                return Err(Error::InvalidPath {
                    path: path.to_path_buf(),
                    reason,
                });
            }
            safety::SafetyCheck::Dangerous {
                reason,
                can_override: true,
            } => {
                if !args.force {
                    eprintln!("rmx: warning: {}", reason);
                }
            }
        }
    }

    if !args.recursive {
        return Err(Error::InvalidPath {
            path: path.to_path_buf(),
            reason: "Is a directory (use -r to remove)".to_string(),
        });
    }

    if args.dry_run {
        return dry_run_directory(path, args);
    }

    if !args.force {
        let tree =
            tree::discover_tree(path).map_err(|e| Error::io_with_path(path.to_path_buf(), e))?;
        let dir_count = tree.dirs.len();
        let file_count = tree.file_count;

        println!(
            "rmx: descend into directory '{}' ({} files, {} directories)?",
            path.display(),
            file_count,
            dir_count
        );

        if !confirm_yes()? {
            return Ok(DeletionStats::default());
        }
    }

    delete_directory(path, args)
}

fn dry_run_directory(path: &Path, args: &Args) -> Result<DeletionStats, Error> {
    let tree = tree::discover_tree(path).map_err(|e| Error::io_with_path(path.to_path_buf(), e))?;

    if args.verbose {
        println!(
            "would remove '{}' ({} files, {} directories)",
            path.display(),
            tree.file_count,
            tree.dirs.len()
        );
    }

    Ok(DeletionStats {
        dirs_deleted: tree.dirs.len(),
        files_deleted: tree.file_count,
        ..Default::default()
    })
}

fn delete_directory(path: &Path, args: &Args) -> Result<DeletionStats, Error> {
    let start = Instant::now();

    if args.verbose {
        println!("scanning '{}'...", path.display());
    }

    let tree = tree::discover_tree(path).map_err(|e| Error::io_with_path(path.to_path_buf(), e))?;

    let dir_count = tree.dirs.len();
    let file_count = tree.file_count;

    let worker_count = args.threads.unwrap_or_else(|| {
        std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(4)
    });

    let (broker, tx, rx) = Broker::new(tree);
    let broker = Arc::new(broker);

    let error_tracker = Arc::new(worker::ErrorTracker::new());
    let worker_config = worker::WorkerConfig {
        verbose: args.verbose,
        ignore_errors: true,
    };

    let handles = worker::spawn_workers(
        worker_count,
        rx,
        broker.clone(),
        worker_config,
        error_tracker.clone(),
    );
    drop(tx);

    let progress_handle = if args.verbose && dir_count > 10 {
        let total = broker.total_dirs();
        let broker_clone = broker.clone();
        Some(std::thread::spawn(move || loop {
            std::thread::sleep(std::time::Duration::from_millis(200));
            let completed = broker_clone.completed_count();
            if completed >= total {
                break;
            }
            let pct = (completed as f64 / total as f64 * 100.0) as u32;
            eprint!("\rdeleting... {}%", pct);
            std::io::stderr().flush().ok();
        }))
    } else {
        None
    };

    for handle in handles {
        handle.join().expect("Worker thread panicked");
    }

    if let Some(handle) = progress_handle {
        handle.join().ok();
        eprintln!("\rdeleting... done");
    }

    let elapsed = start.elapsed();
    let failures = error_tracker.get_failures();

    if args.verbose {
        println!(
            "removed '{}' ({} files, {} dirs in {:.2?})",
            path.display(),
            file_count,
            dir_count,
            elapsed
        );
    }

    if !failures.is_empty() {
        if args.verbose {
            for failure in failures.iter().take(5) {
                eprintln!(
                    "rmx: cannot remove '{}': {}",
                    failure.path.display(),
                    failure.error
                );
            }
            if failures.len() > 5 {
                eprintln!("rmx: ... and {} more errors", failures.len() - 5);
            }
        }

        return Err(Error::PartialFailure {
            total: dir_count + file_count,
            failed: failures.len(),
            errors: failures,
        });
    }

    Ok(DeletionStats {
        dirs_deleted: dir_count,
        files_deleted: file_count,
        total_time: elapsed,
    })
}

fn confirm_deletion(path: &Path, is_dir: bool) -> Result<bool, Error> {
    let type_str = if is_dir { "directory" } else { "file" };
    eprint!("rmx: remove {} '{}'? ", type_str, path.display());
    std::io::stderr().flush().ok();
    confirm_yes()
}

fn confirm_yes() -> Result<bool, Error> {
    let mut response = String::new();
    std::io::stdin()
        .read_line(&mut response)
        .map_err(|e| Error::Io {
            path: None,
            source: e,
        })?;

    let response = response.trim().to_lowercase();
    Ok(response == "y" || response == "yes")
}
