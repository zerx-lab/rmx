#[cfg(windows)]
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

use clap::{Parser, Subcommand};
use rmx::{broker::Broker, error::Error, safety, tree, worker};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process;
use std::sync::Arc;
use std::thread;
use std::time::Instant;

#[cfg(windows)]
use rmx::progress_ui::{self, DeleteProgress};

#[cfg(windows)]
const SETTINGS_REG_KEY: &str = "Software\\rmx\\Settings";
#[cfg(windows)]
const SKIP_CONFIRM_VALUE: &str = "SkipDeleteConfirm";

const APP_VERSION: &str = env!("APP_VERSION");

#[derive(Parser, Debug)]
#[command(name = "rmx")]
#[command(version = APP_VERSION)]
#[command(about = "Fast parallel file/directory deletion for Windows (rm-compatible)")]
#[command(after_help = "EXAMPLES:\n  \
  rmx file.txt                    Delete file (with confirmation)\n  \
  rmx -f file.txt                 Force delete file (no confirmation)\n  \
  rmx -r ./node_modules           Delete directory (with confirmation)\n  \
  rmx -rf ./target                Force delete directory (no confirmation)\n  \
  rmx -rfv ./dist                 Force delete with verbose output\n  \
  rmx -rf dir1 dir2 dir3          Delete multiple directories\n  \
   rmx init                        Initialize rmx shell extension (install/reinstall)\n  \
   rmx uninstall                   Remove rmx shell extension")]
struct Args {
    #[command(subcommand)]
    command: Option<Command>,

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

    #[arg(
        long = "kill-processes",
        help = "Kill processes that are locking files (use with caution)"
    )]
    kill_processes: bool,

    #[arg(long = "gui", help = "Show GUI progress window (used by context menu)")]
    gui: bool,

    #[arg(
        long = "unlock",
        help = "Only unlock files/directories (close handles) without deleting"
    )]
    unlock: bool,

    #[arg(
        long = "reset-confirm",
        help = "Reset skip-confirmation setting, restore delete confirmation dialog"
    )]
    reset_confirm: bool,
}

#[derive(Subcommand, Debug)]
enum Command {
    #[command(
        about = "Initialize rmx shell extension - install or reinstall context menu handler"
    )]
    Init,
    #[command(about = "Remove rmx shell extension and context menu handler")]
    Uninstall,
    #[command(about = "Upgrade rmx to the latest version from GitHub Releases")]
    Upgrade {
        #[arg(long, help = "Only check for updates without installing")]
        check: bool,
        #[arg(
            short = 'f',
            long,
            help = "Force upgrade, bypass package manager detection"
        )]
        force: bool,
    },
}

fn main() {
    rmx::upgrade::cleanup_old_binary();
    let args = Args::parse();

    #[cfg(windows)]
    if args.gui {
        unsafe {
            let _ = windows::Win32::System::Console::FreeConsole();
        }
    }

    if let Some(command) = args.command {
        if let Err(e) = run_command(command) {
            eprintln!("rmx: {}", e);
            process::exit(1);
        }
        return;
    }

    #[cfg(windows)]
    if args.reset_confirm {
        write_skip_confirm(false);
        println!("rmx: delete confirmation dialog has been restored.");
        return;
    }

    #[cfg(not(windows))]
    if args.reset_confirm {
        println!("rmx: --reset-confirm is only available on Windows.");
        return;
    }

    if args.paths.is_empty() {
        eprintln!("rmx: missing operand");
        eprintln!("Try 'rmx --help' for more information.");
        process::exit(1);
    }

    if args.unlock {
        if let Err(e) = run_unlock(&args) {
            eprintln!("rmx: {}", e);
            process::exit(1);
        }
        return;
    }

    if let Err(e) = run(args) {
        eprintln!("rmx: {}", e);
        process::exit(e.exit_code());
    }
}

#[cfg(windows)]
fn run_command(command: Command) -> Result<(), std::io::Error> {
    use rmx::context_menu;

    match command {
        Command::Init => {
            context_menu::init()?;
            println!("rmx shell extension has been initialized.");
            println!("Right-click on any file or folder to see 'Delete with rmx'.");
            Ok(())
        }
        Command::Uninstall => {
            context_menu::uninstall()?;
            println!("rmx shell extension has been removed.");
            Ok(())
        }
        Command::Upgrade { check, force } => rmx::upgrade::run_upgrade(check, force)
            .map_err(|e| std::io::Error::other(e.to_string())),
    }
}

#[cfg(not(windows))]
fn run_command(command: Command) -> Result<(), std::io::Error> {
    match command {
        Command::Upgrade { check, force } => rmx::upgrade::run_upgrade(check, force)
            .map_err(|e| std::io::Error::other(e.to_string())),
        _ => Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "Shell extension is only available on Windows",
        )),
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
    total_bytes: u64,
    total_time: std::time::Duration,
}

impl DeletionStats {
    fn merge(&mut self, other: &DeletionStats) {
        self.dirs_deleted += other.dirs_deleted;
        self.files_deleted += other.files_deleted;
        self.total_bytes += other.total_bytes;
        self.total_time += other.total_time;
    }

    fn total_items(&self) -> usize {
        self.dirs_deleted + self.files_deleted
    }
}

fn format_bytes(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;
    const TB: u64 = GB * 1024;

    if bytes >= TB {
        format!("{:.2} TB", bytes as f64 / TB as f64)
    } else if bytes >= GB {
        format!("{:.2} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.2} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.2} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

fn print_summary(stats: &DeletionStats, args: &Args) {
    if args.stats {
        println!("\nStatistics:");
        println!("  Directories: {}", stats.dirs_deleted);
        println!("  Files:       {}", stats.files_deleted);
        println!("  Total:       {}", stats.total_items());
        println!("  Size:        {}", format_bytes(stats.total_bytes));
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
        #[cfg(windows)]
        if args.gui {
            if !read_skip_confirm() {
                let result = progress_ui::run_confirmation_dialog(path.to_path_buf(), 1, 0)
                    .unwrap_or(progress_ui::ConfirmResult {
                        confirmed: false,
                        skip_next_confirm: false,
                    });

                if result.confirmed && result.skip_next_confirm {
                    write_skip_confirm(true);
                }

                if !result.confirmed {
                    return Ok(DeletionStats::default());
                }
            }
        } else if !confirm_deletion(path, false)? {
            return Ok(DeletionStats::default());
        }

        #[cfg(not(windows))]
        if !confirm_deletion(path, false)? {
            return Ok(DeletionStats::default());
        }
    }

    let start = Instant::now();

    match rmx::winapi::delete_file(path) {
        Ok(()) => {}
        Err(e) if args.kill_processes && rmx::winapi::is_file_in_use_error(&e) => {
            // Step 1: Restart Manager — 精准找到并杀掉占用进程（快速可靠）
            let _ = rmx::winapi::kill_locking_processes(path, args.verbose);
            if rmx::winapi::delete_file(path).is_err() {
                // Step 2: 暴力句柄扫描兜底（慢，但能处理 RM 找不到的情况）
                let paths = [path.to_path_buf()];
                let _ = rmx::winapi::force_close_file_handles(&paths, args.verbose);
                rmx::winapi::delete_file(path)
                    .map_err(|e2| Error::io_with_path(path.to_path_buf(), e2))?;
            }
        }
        Err(e) => {
            return Err(Error::io_with_path(path.to_path_buf(), e));
        }
    }

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

        #[cfg(windows)]
        if args.gui {
            if !read_skip_confirm() {
                let result =
                    progress_ui::run_confirmation_dialog(path.to_path_buf(), file_count, dir_count)
                        .unwrap_or(progress_ui::ConfirmResult {
                            confirmed: false,
                            skip_next_confirm: false,
                        });

                if result.confirmed && result.skip_next_confirm {
                    write_skip_confirm(true);
                }

                if !result.confirmed {
                    return Ok(DeletionStats::default());
                }
            }
            return delete_directory(path, args, Some(tree));
        } else {
            eprint!(
                "rmx: descend into directory '{}' ({} files, {} directories)? [y/N] ",
                path.display(),
                file_count,
                dir_count
            );
            std::io::stderr().flush().ok();

            if !confirm_yes()? {
                return Ok(DeletionStats::default());
            }
        }

        #[cfg(not(windows))]
        {
            eprint!(
                "rmx: descend into directory '{}' ({} files, {} directories)? [y/N] ",
                path.display(),
                file_count,
                dir_count
            );
            std::io::stderr().flush().ok();

            if !confirm_yes()? {
                return Ok(DeletionStats::default());
            }
        }

        return delete_directory(path, args, Some(tree));
    }

    delete_directory(path, args, None)
}

fn dry_run_directory(path: &Path, args: &Args) -> Result<DeletionStats, Error> {
    let tree = tree::discover_tree(path).map_err(|e| Error::io_with_path(path.to_path_buf(), e))?;

    if args.verbose {
        println!(
            "would remove '{}' ({} files, {} directories, {})",
            path.display(),
            tree.file_count,
            tree.dirs.len(),
            format_bytes(tree.total_bytes)
        );
    }

    Ok(DeletionStats {
        dirs_deleted: tree.dirs.len(),
        files_deleted: tree.file_count,
        total_bytes: tree.total_bytes,
        ..Default::default()
    })
}

fn delete_directory(
    path: &Path,
    args: &Args,
    cached_tree: Option<tree::DirectoryTree>,
) -> Result<DeletionStats, Error> {
    #[cfg(windows)]
    if args.gui {
        return delete_directory_with_gui(path, args, cached_tree);
    }

    delete_directory_internal(path, args, None, cached_tree)
}

#[cfg(windows)]
fn delete_directory_with_gui(
    path: &Path,
    args: &Args,
    cached_tree: Option<tree::DirectoryTree>,
) -> Result<DeletionStats, Error> {
    let tree = match cached_tree {
        Some(t) => t,
        None => {
            tree::discover_tree(path).map_err(|e| Error::io_with_path(path.to_path_buf(), e))?
        }
    };

    let total_items = tree.file_count + tree.dirs.len();

    if !progress_ui::should_show_progress_ui(total_items) {
        return delete_directory_internal(path, args, None, Some(tree));
    }

    let progress = Arc::new(DeleteProgress::new(tree.file_count, tree.dirs.len()));
    let progress_clone = progress.clone();
    let path_buf = path.to_path_buf();
    let args_clone = Args {
        command: None,
        paths: vec![],
        force: args.force,
        recursive: args.recursive,
        threads: args.threads,
        dry_run: args.dry_run,
        verbose: args.verbose,
        stats: args.stats,
        no_preserve_root: args.no_preserve_root,
        kill_processes: args.kill_processes,
        gui: false,
        unlock: false,
        reset_confirm: false,
    };

    let delete_handle = thread::spawn(move || {
        let result = delete_directory_internal(
            &path_buf,
            &args_clone,
            Some(progress_clone.clone()),
            Some(tree),
        );

        match &result {
            Ok(_) => {
                progress_clone.set_errors(Vec::new());
            }
            Err(Error::PartialFailure { errors, .. }) => {
                let error_messages: Vec<String> = errors
                    .iter()
                    .map(|e| format!("{}: {}", e.path.display(), e.error))
                    .collect();
                progress_clone.set_errors(error_messages);
            }
            Err(e) => {
                progress_clone.set_errors(vec![e.to_string()]);
            }
        }

        progress_clone.mark_complete();
        result
    });

    let _ = progress_ui::run_progress_window(progress.clone(), path.to_path_buf());

    match delete_handle.join() {
        Ok(result) => result,
        Err(_) => {
            progress.set_errors(vec!["Delete thread panicked".to_string()]);
            progress.mark_complete();
            Err(Error::InvalidPath {
                path: path.to_path_buf(),
                reason: "Delete thread panicked".to_string(),
            })
        }
    }
}

fn delete_directory_internal(
    path: &Path,
    args: &Args,
    #[allow(unused_variables)] progress: Option<Arc<DeleteProgress>>,
    cached_tree: Option<tree::DirectoryTree>,
) -> Result<DeletionStats, Error> {
    let start = Instant::now();

    let tree = match cached_tree {
        Some(t) => {
            if args.verbose {
                println!("reusing cached tree for '{}'...", path.display());
            }
            t
        }
        None => {
            if args.verbose {
                println!("scanning '{}'...", path.display());
            }
            tree::discover_tree(path).map_err(|e| Error::io_with_path(path.to_path_buf(), e))?
        }
    };

    let dir_count = tree.dirs.len();
    let file_count = tree.file_count;
    let total_bytes = tree.total_bytes;

    let worker_count = args.threads.unwrap_or_else(tree::cpu_count);

    let (broker, tx, rx) = Broker::new(tree);
    let broker = Arc::new(broker);

    let error_tracker = Arc::new(worker::ErrorTracker::new());
    let worker_config = worker::WorkerConfig {
        verbose: args.verbose,
        ignore_errors: true,
        kill_processes: args.kill_processes,
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
        Some(thread::spawn(move || loop {
            thread::sleep(std::time::Duration::from_millis(200));
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

    #[cfg(windows)]
    let gui_progress_handle = progress.as_ref().map(|p| {
        let progress = p.clone();
        let broker_clone = broker.clone();
        let total = broker_clone.total_dirs();
        thread::spawn(move || loop {
            thread::sleep(std::time::Duration::from_millis(50));
            let completed = broker_clone.completed_count();
            progress
                .deleted_dirs
                .store(completed, std::sync::atomic::Ordering::Relaxed);

            if completed >= total
                || progress.is_cancelled()
                || progress
                    .is_complete
                    .load(std::sync::atomic::Ordering::Acquire)
            {
                let final_completed = broker_clone.completed_count();
                progress
                    .deleted_dirs
                    .store(final_completed, std::sync::atomic::Ordering::Relaxed);
                break;
            }
        })
    });

    for handle in handles {
        handle.join().expect("Worker thread panicked");
    }

    if let Some(handle) = progress_handle {
        handle.join().ok();
        eprintln!("\rdeleting... done");
    }

    let elapsed = start.elapsed();
    let failures = error_tracker.get_failures();

    #[cfg(windows)]
    if let Some(ref p) = progress {
        p.deleted_dirs.store(
            broker.completed_count(),
            std::sync::atomic::Ordering::Relaxed,
        );
        if !failures.is_empty() {
            let error_messages: Vec<String> = failures
                .iter()
                .map(|e| format!("{}: {}", e.path.display(), e.error))
                .collect();
            p.set_errors(error_messages);
        }
        p.mark_complete();
    }

    #[cfg(windows)]
    if let Some(handle) = gui_progress_handle {
        handle.join().ok();
    }

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
        total_bytes,
        total_time: elapsed,
    })
}

#[cfg(windows)]
fn read_skip_confirm() -> bool {
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::*;
    use windows::Win32::System::Registry::*;

    let key_wide: Vec<u16> = SETTINGS_REG_KEY
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    let value_wide: Vec<u16> = SKIP_CONFIRM_VALUE
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();

    unsafe {
        let mut hkey = HKEY::default();
        let result = RegOpenKeyExW(
            HKEY_CURRENT_USER,
            PCWSTR(key_wide.as_ptr()),
            0,
            KEY_READ,
            &mut hkey,
        );
        if result != ERROR_SUCCESS {
            return false;
        }

        let mut data: u32 = 0;
        let mut data_size = std::mem::size_of::<u32>() as u32;
        let result = RegQueryValueExW(
            hkey,
            PCWSTR(value_wide.as_ptr()),
            None,
            None,
            Some(&mut data as *mut u32 as *mut u8),
            Some(&mut data_size),
        );
        let _ = RegCloseKey(hkey);

        result == ERROR_SUCCESS && data != 0
    }
}

#[cfg(windows)]
fn write_skip_confirm(skip: bool) {
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::*;
    use windows::Win32::System::Registry::*;

    let key_wide: Vec<u16> = SETTINGS_REG_KEY
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    let value_wide: Vec<u16> = SKIP_CONFIRM_VALUE
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();

    unsafe {
        let mut hkey = HKEY::default();
        let result = RegCreateKeyExW(
            HKEY_CURRENT_USER,
            PCWSTR(key_wide.as_ptr()),
            0,
            PCWSTR::null(),
            REG_OPTION_NON_VOLATILE,
            KEY_WRITE,
            None,
            &mut hkey,
            None,
        );
        if result != ERROR_SUCCESS {
            return;
        }

        let data: u32 = if skip { 1 } else { 0 };
        let _ = RegSetValueExW(
            hkey,
            PCWSTR(value_wide.as_ptr()),
            0,
            REG_DWORD,
            Some(std::slice::from_raw_parts(
                &data as *const u32 as *const u8,
                std::mem::size_of::<u32>(),
            )),
        );
        let _ = RegCloseKey(hkey);
    }
}

fn confirm_deletion(path: &Path, is_dir: bool) -> Result<bool, Error> {
    let type_str = if is_dir { "directory" } else { "file" };
    eprint!("rmx: remove {} '{}'? [y/N] ", type_str, path.display());
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

// ── Unlock mode ──────────────────────────────────────────────────────────

fn run_unlock(args: &Args) -> Result<(), Error> {
    let verbose = args.verbose;

    for path in &args.paths {
        let exists = rmx::winapi::path_exists(path);
        if !exists {
            eprintln!(
                "rmx: cannot access '{}': No such file or directory",
                path.display()
            );
            continue;
        }

        let is_dir = rmx::winapi::is_directory(path);
        if is_dir {
            #[cfg(windows)]
            if args.gui {
                unlock_directory_gui(path)?;
            } else {
                unlock_directory(path, verbose)?;
            }

            #[cfg(not(windows))]
            unlock_directory(path, verbose)?;
        } else {
            #[cfg(windows)]
            if args.gui {
                unlock_single_file_gui(path)?;
            } else {
                unlock_single_file(path, verbose)?;
            }

            #[cfg(not(windows))]
            unlock_single_file(path, verbose)?;
        }
    }

    Ok(())
}

#[cfg(windows)]
fn unlock_directory_gui(path: &Path) -> Result<(), Error> {
    let tree = tree::discover_tree(path).map_err(|e| Error::io_with_path(path.to_path_buf(), e))?;

    let mut all_files: Vec<PathBuf> = Vec::new();
    for files in tree.dir_files.values() {
        all_files.extend(files.iter().cloned());
    }

    let mut all_dirs: Vec<PathBuf> = tree.dirs.clone();
    all_dirs.push(path.to_path_buf());

    let total_items = all_files.len() + all_dirs.len();
    if total_items == 0 {
        return Ok(());
    }

    let mut all_locking_procs: Vec<rmx::winapi::LockingProcess> = Vec::new();

    #[cfg(windows)]
    {
        if !all_files.is_empty() {
            if let Ok(procs) = rmx::winapi::find_locking_processes_batch(&all_files) {
                all_locking_procs.extend(procs);
            }
        }

        if !all_dirs.is_empty() {
            if let Ok(procs) = rmx::winapi::find_locking_processes_batch(&all_dirs) {
                all_locking_procs.extend(procs);
            }
        }

        all_locking_procs.sort_by(|a, b| a.pid.cmp(&b.pid));
        all_locking_procs.dedup_by(|a, b| a.pid == b.pid);
    }

    let file_infos = vec![progress_ui::UnlockFileInfo {
        file_name: path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| path.display().to_string()),
        full_path: path.to_path_buf(),
    }];

    let _ = progress_ui::run_unlock_dialog(path.to_path_buf(), file_infos, all_locking_procs);

    Ok(())
}

#[cfg(windows)]
fn unlock_single_file_gui(path: &Path) -> Result<(), Error> {
    let locking_processes = rmx::winapi::find_locking_processes(path).unwrap_or_default();

    let file_infos = vec![progress_ui::UnlockFileInfo {
        file_name: path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| path.display().to_string()),
        full_path: path.to_path_buf(),
    }];

    let _ = progress_ui::run_unlock_dialog(path.to_path_buf(), file_infos, locking_processes);

    Ok(())
}

fn unlock_single_file(path: &Path, verbose: bool) -> Result<(), Error> {
    if verbose {
        println!("unlocking '{}'...", path.display());
    }

    match rmx::winapi::kill_locking_processes(path, verbose) {
        Ok(killed) if !killed.is_empty() => {
            for p in &killed {
                println!("  killed '{}' (PID {})", p.name, p.pid);
            }
        }
        _ => {}
    }

    let paths = [path.to_path_buf()];
    match rmx::winapi::force_close_file_handles(&paths, verbose) {
        Ok(count) if count > 0 => {
            println!("  closed {} handle(s) for '{}'", count, path.display());
        }
        _ => {
            if verbose {
                println!("  no locks found for '{}'", path.display());
            }
        }
    }

    Ok(())
}

fn unlock_directory(path: &Path, verbose: bool) -> Result<(), Error> {
    println!("unlocking directory '{}'...", path.display());

    let tree = tree::discover_tree(path).map_err(|e| Error::io_with_path(path.to_path_buf(), e))?;

    let mut all_files: Vec<PathBuf> = Vec::new();
    for files in tree.dir_files.values() {
        all_files.extend(files.iter().cloned());
    }

    let mut all_dirs: Vec<PathBuf> = tree.dirs.clone();
    all_dirs.push(path.to_path_buf());

    let total_items = all_files.len() + all_dirs.len();
    println!(
        "  scanning complete: {} files, {} directories",
        all_files.len(),
        all_dirs.len()
    );

    if total_items == 0 {
        println!("  nothing to unlock");
        return Ok(());
    }

    let mut total_killed = 0usize;
    let mut total_handles_closed = 0usize;

    if !all_files.is_empty() {
        match rmx::winapi::kill_locking_processes_batch(&all_files, verbose) {
            Ok(killed) => {
                for p in &killed {
                    if verbose {
                        println!("  killed '{}' (PID {})", p.name, p.pid);
                    }
                }
                total_killed += killed.len();
            }
            Err(e) => {
                if verbose {
                    eprintln!("  warning: batch process kill failed: {}", e);
                }
            }
        }
    }

    if !all_dirs.is_empty() {
        match rmx::winapi::kill_locking_processes_batch(&all_dirs, verbose) {
            Ok(killed) => {
                for p in &killed {
                    if verbose {
                        println!("  killed '{}' (PID {})", p.name, p.pid);
                    }
                }
                total_killed += killed.len();
            }
            Err(e) => {
                if verbose {
                    eprintln!("  warning: batch directory process kill failed: {}", e);
                }
            }
        }
    }

    let mut all_paths: Vec<PathBuf> = Vec::with_capacity(all_files.len() + all_dirs.len());
    all_paths.extend(all_files);
    all_paths.extend(all_dirs);

    match rmx::winapi::force_close_file_handles(&all_paths, verbose) {
        Ok(count) => {
            total_handles_closed += count;
        }
        Err(e) => {
            if verbose {
                eprintln!("  warning: force close handles failed: {}", e);
            }
        }
    }

    println!(
        "  done: killed {} process(es), closed {} handle(s)",
        total_killed, total_handles_closed
    );

    Ok(())
}
