// Hide console window when launched from GUI (e.g., context menu)
// CLI usage will attach to parent console via AttachConsole
#![windows_subsystem = "windows"]

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
  rmx install                     Add rmx to right-click context menu\n  \
  rmx uninstall                   Remove rmx from right-click context menu")]
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
}

#[derive(Subcommand, Debug)]
enum Command {
    #[command(about = "Add rmx to the Windows Explorer right-click context menu (requires admin)")]
    Install,
    #[command(
        about = "Remove rmx from the Windows Explorer right-click context menu (requires admin)"
    )]
    Uninstall,
}

/// Setup console I/O for Windows GUI subsystem application.
///
/// This handles three scenarios:
/// 1. Launched from terminal (cmd/powershell): attach to parent console, output visible
/// 2. Launched from terminal with redirection (e.g., `rmx --help > out.txt`): use existing pipe/file handles
/// 3. Launched from GUI (e.g., Explorer right-click): no console, no popup window
#[cfg(windows)]
unsafe fn setup_console() {
    use std::os::raw::{c_char, c_int, c_void};
    use windows::Win32::Foundation::INVALID_HANDLE_VALUE;
    use windows::Win32::Storage::FileSystem::{
        CreateFileW, GetFileType, FILE_GENERIC_READ, FILE_GENERIC_WRITE, FILE_SHARE_READ,
        FILE_SHARE_WRITE, OPEN_EXISTING,
    };
    use windows::Win32::System::Console::{
        AttachConsole, GetStdHandle, SetStdHandle, ATTACH_PARENT_PROCESS, STD_ERROR_HANDLE,
        STD_INPUT_HANDLE, STD_OUTPUT_HANDLE,
    };

    const STDIN_FILENO: c_int = 0;
    const STDOUT_FILENO: c_int = 1;
    const STDERR_FILENO: c_int = 2;

    extern "C" {
        fn freopen(
            filename: *const c_char,
            mode: *const c_char,
            stream: *mut c_void,
        ) -> *mut c_void;
        fn __acrt_iob_func(index: c_int) -> *mut c_void;
    }

    // Check if stdout is already valid (redirected to pipe/file)
    // For GUI subsystem apps, handles are typically NULL unless redirected
    let stdout_handle = GetStdHandle(STD_OUTPUT_HANDLE).unwrap_or(INVALID_HANDLE_VALUE);
    let stdout_valid = !stdout_handle.is_invalid()
        && !stdout_handle.0.is_null()
        && GetFileType(stdout_handle).0 != 0;

    if stdout_valid {
        // stdout already has a valid handle (pipe/file redirection)
        // Don't attach to console, just use existing handles
        return;
    }

    // No valid stdout - try to attach to parent console (terminal scenario)
    if AttachConsole(ATTACH_PARENT_PROCESS).is_err() {
        // No parent console (launched from GUI) - stay silent, no popup
        return;
    }

    // Successfully attached to parent console
    // Now setup both Windows handles (for Rust std::io) and C stdio

    // Open CONOUT$ and CONIN$ for Windows handles
    let conout: Vec<u16> = "CONOUT$\0".encode_utf16().collect();
    let conin: Vec<u16> = "CONIN$\0".encode_utf16().collect();

    // Setup stdout/stderr with CONOUT$
    if let Ok(h) = CreateFileW(
        windows::core::PCWSTR(conout.as_ptr()),
        FILE_GENERIC_WRITE.0,
        FILE_SHARE_WRITE,
        None,
        OPEN_EXISTING,
        Default::default(),
        None,
    ) {
        let _ = SetStdHandle(STD_OUTPUT_HANDLE, h);
        let _ = SetStdHandle(STD_ERROR_HANDLE, h);
    }

    // Setup stdin with CONIN$
    if let Ok(h) = CreateFileW(
        windows::core::PCWSTR(conin.as_ptr()),
        FILE_GENERIC_READ.0,
        FILE_SHARE_READ,
        None,
        OPEN_EXISTING,
        Default::default(),
        None,
    ) {
        let _ = SetStdHandle(STD_INPUT_HANDLE, h);
    }

    // Also setup C stdio (for any C library code that might use printf, etc.)
    let conout_c = c"CONOUT$".as_ptr();
    let conin_c = c"CONIN$".as_ptr();
    let w = c"w".as_ptr();
    let r = c"r".as_ptr();
    freopen(conin_c, r, __acrt_iob_func(STDIN_FILENO));
    freopen(conout_c, w, __acrt_iob_func(STDOUT_FILENO));
    freopen(conout_c, w, __acrt_iob_func(STDERR_FILENO));
}

fn main() {
    #[cfg(windows)]
    unsafe {
        setup_console();
    }

    let args = Args::parse();

    if let Some(command) = args.command {
        if let Err(e) = run_command(command) {
            eprintln!("rmx: {}", e);
            process::exit(1);
        }
        return;
    }

    if args.paths.is_empty() {
        eprintln!("rmx: missing operand");
        eprintln!("Try 'rmx --help' for more information.");
        process::exit(1);
    }

    if let Err(e) = run(args) {
        eprintln!("rmx: {}", e);
        process::exit(e.exit_code());
    }
}

#[cfg(windows)]
fn run_command(command: Command) -> Result<(), std::io::Error> {
    use rmx::context_menu;
    use std::io::{Error, ErrorKind};

    match command {
        Command::Install => match context_menu::install() {
            Ok(()) => {
                println!("rmx has been added to the context menu.");
                println!("Right-click on any folder to see 'Delete with rmx'.");
                Ok(())
            }
            Err(e) if e.kind() == ErrorKind::PermissionDenied => Err(Error::new(
                ErrorKind::PermissionDenied,
                "Administrator privileges required. Run as administrator.",
            )),
            Err(e) => Err(e),
        },
        Command::Uninstall => match context_menu::uninstall() {
            Ok(()) => {
                println!("rmx has been removed from the context menu.");
                Ok(())
            }
            Err(e) if e.kind() == ErrorKind::PermissionDenied => Err(Error::new(
                ErrorKind::PermissionDenied,
                "Administrator privileges required. Run as administrator.",
            )),
            Err(e) => Err(e),
        },
    }
}

#[cfg(not(windows))]
fn run_command(_command: Command) -> Result<(), std::io::Error> {
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "Context menu integration is only available on Windows",
    ))
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

    if !args.force && !confirm_deletion(path, false)? {
        return Ok(DeletionStats::default());
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

    delete_directory(path, args)
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

fn delete_directory(path: &Path, args: &Args) -> Result<DeletionStats, Error> {
    #[cfg(windows)]
    if args.gui {
        return delete_directory_with_gui(path, args);
    }

    delete_directory_internal(path, args, None)
}

#[cfg(windows)]
fn delete_directory_with_gui(path: &Path, args: &Args) -> Result<DeletionStats, Error> {
    let tree = tree::discover_tree(path).map_err(|e| Error::io_with_path(path.to_path_buf(), e))?;

    let total_items = tree.file_count + tree.dirs.len();

    if !progress_ui::should_show_progress_ui(total_items) {
        return delete_directory_internal(path, args, None);
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
    };

    let delete_handle = thread::spawn(move || {
        let result =
            delete_directory_internal(&path_buf, &args_clone, Some(progress_clone.clone()));

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
) -> Result<DeletionStats, Error> {
    let start = Instant::now();

    if args.verbose {
        println!("scanning '{}'...", path.display());
    }

    let tree = tree::discover_tree(path).map_err(|e| Error::io_with_path(path.to_path_buf(), e))?;

    let dir_count = tree.dirs.len();
    let file_count = tree.file_count;
    let total_bytes = tree.total_bytes;

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
                || progress.is_complete.load(std::sync::atomic::Ordering::Acquire)
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
        p.deleted_dirs
            .store(broker.completed_count(), std::sync::atomic::Ordering::Relaxed);
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
