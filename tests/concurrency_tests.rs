use std::fs;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

fn rmx_path() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_rmx"))
}

fn create_test_dir(name: &str) -> PathBuf {
    let temp = std::env::temp_dir().join(format!("rmx_conc_{}", name));
    let _ = fs::remove_dir_all(&temp);
    fs::create_dir_all(&temp).unwrap();
    temp
}

fn cleanup(path: &PathBuf) {
    let _ = fs::remove_dir_all(path);
}

#[test]
fn concurrency_file_locking() {
    let test_dir = create_test_dir("file_locking");

    for i in 0..10 {
        let dir = test_dir.join(format!("dir-{}", i));
        fs::create_dir_all(&dir).unwrap();
        for j in 0..20 {
            fs::write(dir.join(format!("file-{}.txt", j)), "content").unwrap();
        }
    }

    let locked_file = test_dir.join("dir-5").join("file-10.txt");
    let _handle = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(&locked_file)
        .unwrap();

    let output = Command::new(rmx_path())
        .args(["-rf", "--stats"])
        .arg(&test_dir)
        .output()
        .expect("Failed to execute rmx");

    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);

    println!("=== File Locking Test ===");
    println!("Exit code: {:?}", output.status.code());
    println!("Stdout: {}", stdout);
    println!("Stderr: {}", stderr);

    drop(_handle);
    cleanup(&test_dir);
}

#[test]
fn concurrency_long_paths() {
    let test_dir = create_test_dir("long_paths");

    let long_name = "a".repeat(50);
    let mut current = test_dir.clone();

    for i in 0..8 {
        current = current.join(format!("{}_{}", long_name, i));
        fs::create_dir_all(&current).unwrap();
        fs::write(current.join("file.txt"), "content").unwrap();
    }

    let total_path_len = current.to_string_lossy().len();
    println!("=== Long Paths Test ===");
    println!("Total path length: {} characters", total_path_len);

    assert!(total_path_len > 260, "Path should exceed 260 characters");

    let output = Command::new(rmx_path())
        .args(["-rf", "--stats"])
        .arg(&test_dir)
        .output()
        .expect("Failed to execute rmx");

    assert!(output.status.success(), "Should handle long paths");
    assert!(!test_dir.exists(), "Directory should be deleted");

    println!("Successfully deleted paths > 260 characters");
}

#[test]
fn concurrency_special_characters() {
    let test_dir = create_test_dir("special_chars");

    let special_names = vec![
        "file with spaces.txt",
        "file\twith\ttabs.txt",
        "中文文件名.txt",
        "日本語ファイル.txt",
        "emoji_🎉_file.txt",
        "file-with-dashes.txt",
        "file_with_underscores.txt",
        "file.multiple.dots.txt",
        "UPPERCASE.TXT",
        "MixedCase.Txt",
        "file'with'quotes.txt",
        "file`with`backticks.txt",
        "(parentheses).txt",
        "[brackets].txt",
        "{braces}.txt",
        "file@at.txt",
        "file#hash.txt",
        "file$dollar.txt",
        "file%percent.txt",
        "file&ampersand.txt",
        "file=equals.txt",
        "file+plus.txt",
        "file;semicolon.txt",
        "file,comma.txt",
    ];

    for name in &special_names {
        let path = test_dir.join(name);
        if let Err(e) = fs::write(&path, "content") {
            println!("Could not create '{}': {}", name, e);
        }
    }

    let created_count = fs::read_dir(&test_dir).unwrap().count();
    println!("=== Special Characters Test ===");
    println!("Created {} files with special characters", created_count);

    let output = Command::new(rmx_path())
        .args(["-rf", "--stats"])
        .arg(&test_dir)
        .output()
        .expect("Failed to execute rmx");

    let stdout = String::from_utf8_lossy(&output.stdout);
    println!("Output: {}", stdout);

    assert!(output.status.success(), "Should handle special characters");
    assert!(!test_dir.exists(), "Directory should be deleted");
}

#[test]
fn concurrency_multiple_instances() {
    let base_dir = create_test_dir("multi_instance");

    let mut test_dirs = Vec::new();
    for i in 0..4 {
        let dir = base_dir.join(format!("target-{}", i));
        fs::create_dir_all(&dir).unwrap();
        for j in 0..100 {
            let subdir = dir.join(format!("sub-{}", j));
            fs::create_dir_all(&subdir).unwrap();
            for k in 0..10 {
                fs::write(subdir.join(format!("file-{}.txt", k)), "content").unwrap();
            }
        }
        test_dirs.push(dir);
    }

    println!("=== Multiple Instances Test ===");
    println!("Starting 4 concurrent rmx processes...");

    let start = Instant::now();

    let handles: Vec<Child> = test_dirs
        .iter()
        .map(|dir| {
            Command::new(rmx_path())
                .args(["-rf", "--stats"])
                .arg(dir)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .expect("Failed to spawn rmx")
        })
        .collect();

    let mut all_success = true;
    for mut handle in handles {
        let status = handle.wait().expect("Failed to wait for rmx");
        if !status.success() {
            all_success = false;
        }
    }

    let elapsed = start.elapsed();
    println!("Total time for 4 concurrent deletions: {:.2?}", elapsed);
    println!("All succeeded: {}", all_success);

    for dir in &test_dirs {
        assert!(!dir.exists(), "Directory {:?} should be deleted", dir);
    }

    cleanup(&base_dir);
}

#[test]
fn concurrency_readonly_nested() {
    let test_dir = create_test_dir("readonly_nested");

    for i in 0..20 {
        let dir = test_dir.join(format!("dir-{}", i));
        fs::create_dir_all(&dir).unwrap();

        for j in 0..10 {
            let file = dir.join(format!("file-{}.txt", j));
            fs::write(&file, "content").unwrap();

            if j % 3 == 0 {
                let mut perms = fs::metadata(&file).unwrap().permissions();
                perms.set_readonly(true);
                fs::set_permissions(&file, perms).unwrap();
            }
        }
    }

    println!("=== Readonly Nested Test ===");

    let output = Command::new(rmx_path())
        .args(["-rf", "--stats"])
        .arg(&test_dir)
        .output()
        .expect("Failed to execute rmx");

    let stdout = String::from_utf8_lossy(&output.stdout);
    println!("Output: {}", stdout);

    assert!(output.status.success(), "Should handle readonly files");
    assert!(!test_dir.exists(), "Directory should be deleted");
}

#[test]
#[cfg(windows)]
fn concurrency_symlinks() {
    let test_dir = create_test_dir("symlinks");

    let real_dir = test_dir.join("real_dir");
    fs::create_dir_all(&real_dir).unwrap();
    fs::write(real_dir.join("real_file.txt"), "real content").unwrap();

    let target_dir = test_dir.join("target_with_link");
    fs::create_dir_all(&target_dir).unwrap();
    fs::write(target_dir.join("normal_file.txt"), "normal").unwrap();

    let junction_path = target_dir.join("junction_to_real");

    let output = Command::new("cmd")
        .args(["/C", "mklink", "/J"])
        .arg(&junction_path)
        .arg(&real_dir)
        .output();

    println!("=== Symlinks/Junction Test ===");

    if let Ok(mklink_output) = output {
        if mklink_output.status.success() {
            println!("Created junction point");

            let output = Command::new(rmx_path())
                .args(["-rf", "--stats"])
                .arg(&target_dir)
                .output()
                .expect("Failed to execute rmx");

            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);
            println!("Stdout: {}", stdout);
            println!("Stderr: {}", stderr);

            assert!(!target_dir.exists(), "Target directory should be deleted");
            assert!(
                real_dir.exists(),
                "Real directory should NOT be deleted (junction target)"
            );

            println!("Junction handled correctly - target preserved");
        } else {
            println!("Could not create junction (may need admin rights), skipping");
        }
    }

    cleanup(&test_dir);
}

#[test]
fn concurrency_empty_deep() {
    let test_dir = create_test_dir("empty_deep");

    let mut current = test_dir.clone();
    for i in 0..200 {
        current = current.join(format!("level-{}", i));
        fs::create_dir_all(&current).unwrap();
    }

    println!("=== Empty Deep Directories Test ===");
    println!("Created 200 levels of empty directories");

    let start = Instant::now();
    let output = Command::new(rmx_path())
        .args(["-rf", "--stats"])
        .arg(&test_dir)
        .output()
        .expect("Failed to execute rmx");

    let elapsed = start.elapsed();

    assert!(output.status.success());
    assert!(!test_dir.exists());

    println!("Deleted 200 empty nested directories in {:.2?}", elapsed);
}

#[test]
fn concurrency_mixed_empty() {
    let test_dir = create_test_dir("mixed_empty");

    for i in 0..100 {
        let dir = test_dir.join(format!("dir-{}", i));
        fs::create_dir_all(&dir).unwrap();

        if i % 2 == 0 {
            for j in 0..5 {
                fs::write(dir.join(format!("file-{}.txt", j)), "content").unwrap();
            }
        }

        for k in 0..3 {
            let subdir = dir.join(format!("sub-{}", k));
            fs::create_dir_all(&subdir).unwrap();
            if k == 1 {
                fs::write(subdir.join("nested.txt"), "nested").unwrap();
            }
        }
    }

    println!("=== Mixed Empty/Non-Empty Test ===");

    let output = Command::new(rmx_path())
        .args(["-rf", "--stats"])
        .arg(&test_dir)
        .output()
        .expect("Failed to execute rmx");

    let stdout = String::from_utf8_lossy(&output.stdout);
    println!("Output: {}", stdout);

    assert!(output.status.success());
    assert!(!test_dir.exists());
}

#[test]
fn concurrency_high_contention() {
    let test_dir = create_test_dir("high_contention");

    for i in 0..1000 {
        let dir = test_dir.join(format!("d{}", i));
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("f.txt"), "x").unwrap();
    }

    println!("=== High Contention Test ===");
    println!("1000 directories with 1 file each (worst case for worker scheduling)");

    let start = Instant::now();
    let output = Command::new(rmx_path())
        .args(["-rf", "--stats", "-t", "16"])
        .arg(&test_dir)
        .output()
        .expect("Failed to execute rmx");

    let elapsed = start.elapsed();
    let stdout = String::from_utf8_lossy(&output.stdout);

    println!("Output: {}", stdout);
    println!("Time with 16 threads: {:.2?}", elapsed);

    assert!(output.status.success());
    assert!(!test_dir.exists());
}

#[test]
fn concurrency_partial_failure() {
    let test_dir = create_test_dir("partial_failure");

    for i in 0..20 {
        let dir = test_dir.join(format!("dir-{}", i));
        fs::create_dir_all(&dir).unwrap();
        for j in 0..10 {
            fs::write(dir.join(format!("file-{}.txt", j)), "content").unwrap();
        }
    }

    let locked_dir = test_dir.join("dir-10");
    let locked_file = locked_dir.join("file-5.txt");
    let _handle = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(&locked_file)
        .unwrap();

    println!("=== Partial Failure Recovery Test ===");
    println!("Locked file: {:?}", locked_file);

    let output = Command::new(rmx_path())
        .args(["-rf", "--stats", "-v"])
        .arg(&test_dir)
        .output()
        .expect("Failed to execute rmx");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    println!("Stdout: {}", stdout);
    println!("Stderr: {}", stderr);
    println!("Exit code: {:?}", output.status.code());

    let remaining_dirs: Vec<_> = if test_dir.exists() {
        fs::read_dir(&test_dir)
            .map(|rd| rd.filter_map(|e| e.ok()).collect())
            .unwrap_or_default()
    } else {
        vec![]
    };

    println!("Remaining directories: {}", remaining_dirs.len());

    drop(_handle);
    cleanup(&test_dir);
}

#[test]
fn concurrency_rapid_cycles() {
    let test_dir = create_test_dir("rapid_cycles");

    println!("=== Rapid Create-Delete Cycles Test ===");

    let mut total_time = Duration::ZERO;

    for cycle in 0..5 {
        for i in 0..100 {
            let dir = test_dir.join(format!("dir-{}", i));
            fs::create_dir_all(&dir).unwrap();
            for j in 0..10 {
                fs::write(dir.join(format!("f{}.txt", j)), "x").unwrap();
            }
        }

        let start = Instant::now();
        let output = Command::new(rmx_path())
            .args(["-rf"])
            .arg(&test_dir)
            .output()
            .expect("Failed to execute rmx");

        let elapsed = start.elapsed();
        total_time += elapsed;

        assert!(output.status.success(), "Cycle {} failed", cycle);
        assert!(!test_dir.exists(), "Cycle {} dir still exists", cycle);

        fs::create_dir_all(&test_dir).unwrap();
        println!("  Cycle {}: {:.2?}", cycle, elapsed);
    }

    println!("Total time for 5 cycles: {:.2?}", total_time);
    println!("Average per cycle: {:.2?}", total_time / 5);

    cleanup(&test_dir);
}

#[test]
fn concurrency_thread_scaling() {
    println!("=== Thread Scaling Test ===");

    let thread_counts = [1, 2, 4, 8];

    for &threads in &thread_counts {
        let test_dir = create_test_dir(&format!("thread_scale_{}", threads));

        for i in 0..200 {
            let dir = test_dir.join(format!("dir-{}", i));
            fs::create_dir_all(&dir).unwrap();
            for j in 0..25 {
                fs::write(dir.join(format!("f{}.txt", j)), "content").unwrap();
            }
        }

        let start = Instant::now();
        let output = Command::new(rmx_path())
            .args(["-rf", "-t", &threads.to_string()])
            .arg(&test_dir)
            .output()
            .expect("Failed to execute rmx");

        let elapsed = start.elapsed();

        assert!(output.status.success());
        assert!(!test_dir.exists());

        let throughput = 5200.0 / elapsed.as_secs_f64();
        println!(
            "  {} thread(s): {:.2?} ({:.0} items/sec)",
            threads, elapsed, throughput
        );
    }
}
