use std::fs::{self, File};
use std::io::Write;
use std::path::PathBuf;
use std::process::Command;

fn rmx_path() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_rmx"))
}

fn create_test_dir(name: &str) -> PathBuf {
    let temp = std::env::temp_dir().join(format!("rmx_test_{}", name));
    let _ = fs::remove_dir_all(&temp);
    fs::create_dir_all(&temp).unwrap();
    temp
}

fn create_nested_structure(base: &PathBuf, depth: usize, files_per_dir: usize) {
    let mut current = base.clone();
    for i in 0..depth {
        current = current.join(format!("level{}", i));
        fs::create_dir_all(&current).unwrap();
        for j in 0..files_per_dir {
            let file_path = current.join(format!("file{}.txt", j));
            let mut f = File::create(&file_path).unwrap();
            writeln!(f, "test content {}", j).unwrap();
        }
    }
}

#[test]
fn test_help_command() {
    let output = Command::new(rmx_path())
        .arg("--help")
        .output()
        .expect("Failed to execute rmx");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Fast parallel"));
    assert!(stdout.contains("--dry-run"));
    assert!(stdout.contains("--verbose"));
}

#[test]
fn test_version_command() {
    let output = Command::new(rmx_path())
        .arg("--version")
        .output()
        .expect("Failed to execute rmx");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("rmx"));
}

#[test]
fn test_dry_run_does_not_delete() {
    let test_dir = create_test_dir("dry_run");
    create_nested_structure(&test_dir, 3, 5);

    let output = Command::new(rmx_path())
        .args(["-rfnv"])
        .arg(&test_dir)
        .output()
        .expect("Failed to execute rmx");

    assert!(output.status.success());
    assert!(
        test_dir.exists(),
        "Directory should still exist after dry run"
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("would remove"));

    fs::remove_dir_all(&test_dir).ok();
}

#[test]
fn test_delete_simple_directory() {
    let test_dir = create_test_dir("simple");
    create_nested_structure(&test_dir, 2, 3);

    assert!(test_dir.exists());

    let output = Command::new(rmx_path())
        .args(["-rf"])
        .arg(&test_dir)
        .output()
        .expect("Failed to execute rmx");

    assert!(output.status.success());
    assert!(!test_dir.exists(), "Directory should be deleted");
}

#[test]
fn test_delete_with_verbose() {
    let test_dir = create_test_dir("verbose");
    create_nested_structure(&test_dir, 3, 5);

    let output = Command::new(rmx_path())
        .args(["-rfv", "--stats"])
        .arg(&test_dir)
        .output()
        .expect("Failed to execute rmx");

    assert!(output.status.success());
    assert!(!test_dir.exists());

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("removed") || stdout.contains("Statistics"));
}

#[test]
fn test_nonexistent_path_fails() {
    let output = Command::new(rmx_path())
        .args(["-r"])
        .arg("/nonexistent/path/that/does/not/exist")
        .output()
        .expect("Failed to execute rmx");

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("No such file") || stderr.contains("cannot remove"));
}

#[test]
fn test_force_ignores_nonexistent() {
    let output = Command::new(rmx_path())
        .args(["-rf"])
        .arg("/nonexistent/path/that/does/not/exist")
        .output()
        .expect("Failed to execute rmx");

    assert!(
        output.status.success(),
        "rm -f should silently ignore nonexistent paths"
    );
}

#[test]
fn test_system_directory_protected() {
    #[cfg(windows)]
    {
        let output = Command::new(rmx_path())
            .args(["-rf"])
            .arg("C:\\Windows")
            .output()
            .expect("Failed to execute rmx");

        assert!(!output.status.success());
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains("system directory")
                || stderr.contains("dangerous")
                || stderr.contains("protected")
        );
    }
}

#[test]
fn test_multiple_directories() {
    let dir1 = create_test_dir("multi1");
    let dir2 = create_test_dir("multi2");
    let dir3 = create_test_dir("multi3");

    create_nested_structure(&dir1, 2, 2);
    create_nested_structure(&dir2, 2, 2);
    create_nested_structure(&dir3, 2, 2);

    let output = Command::new(rmx_path())
        .args(["-rf"])
        .arg(&dir1)
        .arg(&dir2)
        .arg(&dir3)
        .output()
        .expect("Failed to execute rmx");

    assert!(output.status.success());
    assert!(!dir1.exists());
    assert!(!dir2.exists());
    assert!(!dir3.exists());
}

#[test]
fn test_custom_thread_count() {
    let test_dir = create_test_dir("threads");
    create_nested_structure(&test_dir, 2, 3);

    let output = Command::new(rmx_path())
        .args(["-rf", "-t", "2"])
        .arg(&test_dir)
        .output()
        .expect("Failed to execute rmx");

    assert!(output.status.success());
    assert!(!test_dir.exists());
}

#[test]
fn test_deep_nesting() {
    let test_dir = create_test_dir("deep");
    create_nested_structure(&test_dir, 20, 2);

    let output = Command::new(rmx_path())
        .args(["-rfv", "--stats"])
        .arg(&test_dir)
        .output()
        .expect("Failed to execute rmx");

    assert!(output.status.success());
    assert!(!test_dir.exists());
}

#[test]
fn test_empty_directory() {
    let test_dir = create_test_dir("empty");

    let output = Command::new(rmx_path())
        .args(["-rf"])
        .arg(&test_dir)
        .output()
        .expect("Failed to execute rmx");

    assert!(output.status.success());
    assert!(!test_dir.exists());
}

#[test]
fn test_readonly_files() {
    let test_dir = create_test_dir("readonly");
    fs::create_dir_all(&test_dir).unwrap();

    let file_path = test_dir.join("readonly.txt");
    {
        let mut f = File::create(&file_path).unwrap();
        writeln!(f, "readonly content").unwrap();
    }

    #[cfg(windows)]
    {
        let mut perms = fs::metadata(&file_path).unwrap().permissions();
        perms.set_readonly(true);
        fs::set_permissions(&file_path, perms).unwrap();
    }

    let output = Command::new(rmx_path())
        .args(["-rf"])
        .arg(&test_dir)
        .output()
        .expect("Failed to execute rmx");

    assert!(output.status.success());
    assert!(
        !test_dir.exists(),
        "Directory with readonly files should be deleted"
    );
}

#[test]
fn test_large_directory() {
    let test_dir = create_test_dir("large");
    fs::create_dir_all(&test_dir).unwrap();

    for i in 0..100 {
        let subdir = test_dir.join(format!("dir{}", i));
        fs::create_dir_all(&subdir).unwrap();
        for j in 0..50 {
            let file_path = subdir.join(format!("file{}.txt", j));
            let mut f = File::create(&file_path).unwrap();
            writeln!(f, "content {} {}", i, j).unwrap();
        }
    }

    let output = Command::new(rmx_path())
        .args(["-rf", "--stats"])
        .arg(&test_dir)
        .output()
        .expect("Failed to execute rmx");

    assert!(output.status.success());
    assert!(!test_dir.exists());

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("5000") || stdout.contains("Files:") || stdout.contains("Total:"));
}

#[test]
fn test_file_deletion() {
    let test_dir = create_test_dir("file_del");
    let file_path = test_dir.join("test.txt");
    {
        let mut f = File::create(&file_path).unwrap();
        writeln!(f, "test content").unwrap();
    }

    let output = Command::new(rmx_path())
        .args(["-f"])
        .arg(&file_path)
        .output()
        .expect("Failed to execute rmx");

    assert!(output.status.success());
    assert!(!file_path.exists(), "File should be deleted");

    fs::remove_dir_all(&test_dir).ok();
}

#[test]
fn test_directory_requires_recursive() {
    let test_dir = create_test_dir("no_recursive");
    create_nested_structure(&test_dir, 2, 2);

    let output = Command::new(rmx_path())
        .args(["-f"])
        .arg(&test_dir)
        .output()
        .expect("Failed to execute rmx");

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("Is a directory") || stderr.contains("-r"));

    assert!(test_dir.exists(), "Directory should still exist");
    fs::remove_dir_all(&test_dir).ok();
}
