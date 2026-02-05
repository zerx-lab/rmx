use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::Instant;

fn rmx_path() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_rmx"))
}

fn create_stress_test_dir(name: &str) -> PathBuf {
    let temp = std::env::temp_dir().join(format!("rmx_stress_{}", name));
    let _ = fs::remove_dir_all(&temp);
    fs::create_dir_all(&temp).unwrap();
    temp
}

struct TestStats {
    dirs: usize,
    files: usize,
    bytes: u64,
}

fn create_node_modules_structure(base: &PathBuf, packages: usize, depth: usize) -> TestStats {
    let mut stats = TestStats {
        dirs: 0,
        files: 0,
        bytes: 0,
    };

    for pkg in 0..packages {
        let pkg_dir = base.join(format!("package-{}", pkg));
        fs::create_dir_all(&pkg_dir).unwrap();
        stats.dirs += 1;

        let pkg_json = pkg_dir.join("package.json");
        let content = format!(r#"{{"name": "package-{}", "version": "1.0.0"}}"#, pkg);
        fs::write(&pkg_json, &content).unwrap();
        stats.files += 1;
        stats.bytes += content.len() as u64;

        let src_dir = pkg_dir.join("src");
        fs::create_dir_all(&src_dir).unwrap();
        stats.dirs += 1;

        for i in 0..5 {
            let file = src_dir.join(format!("index{}.js", i));
            let content = format!("module.exports = function() {{ return {}; }};", i);
            fs::write(&file, &content).unwrap();
            stats.files += 1;
            stats.bytes += content.len() as u64;
        }

        if depth > 0 {
            let nested = pkg_dir.join("node_modules");
            fs::create_dir_all(&nested).unwrap();
            stats.dirs += 1;
            let nested_stats = create_node_modules_structure(&nested, packages / 4 + 1, depth - 1);
            stats.dirs += nested_stats.dirs;
            stats.files += nested_stats.files;
            stats.bytes += nested_stats.bytes;
        }
    }

    stats
}

fn create_target_structure(base: &PathBuf, crates: usize) -> TestStats {
    let mut stats = TestStats {
        dirs: 0,
        files: 0,
        bytes: 0,
    };

    let debug_dir = base.join("debug");
    fs::create_dir_all(&debug_dir).unwrap();
    stats.dirs += 1;

    let deps_dir = debug_dir.join("deps");
    fs::create_dir_all(&deps_dir).unwrap();
    stats.dirs += 1;

    let incremental_dir = debug_dir.join("incremental");
    fs::create_dir_all(&incremental_dir).unwrap();
    stats.dirs += 1;

    let file_size = 10 * 1024;
    let content: Vec<u8> = (0..file_size).map(|i| (i % 256) as u8).collect();

    for i in 0..crates {
        let rlib = deps_dir.join(format!("lib{}.rlib", i));
        fs::write(&rlib, &content).unwrap();
        stats.files += 1;
        stats.bytes += content.len() as u64;

        let d_file = deps_dir.join(format!("lib{}.d", i));
        fs::write(&d_file, format!("deps for {}", i)).unwrap();
        stats.files += 1;
        stats.bytes += 20;
    }

    for i in 0..crates {
        let crate_dir = incremental_dir.join(format!("crate-{}", i));
        fs::create_dir_all(&crate_dir).unwrap();
        stats.dirs += 1;

        for j in 0..10 {
            let dep_graph = crate_dir.join(format!("dep-graph-{}.bin", j));
            fs::write(&dep_graph, &content[..1024]).unwrap();
            stats.files += 1;
            stats.bytes += 1024;
        }
    }

    let build_dir = debug_dir.join("build");
    fs::create_dir_all(&build_dir).unwrap();
    stats.dirs += 1;

    for i in 0..crates / 2 {
        let build_crate = build_dir.join(format!("crate-{}-build", i));
        fs::create_dir_all(&build_crate).unwrap();
        stats.dirs += 1;

        let out_dir = build_crate.join("out");
        fs::create_dir_all(&out_dir).unwrap();
        stats.dirs += 1;

        for j in 0..5 {
            let gen_file = out_dir.join(format!("generated-{}.rs", j));
            fs::write(&gen_file, &content[..2048]).unwrap();
            stats.files += 1;
            stats.bytes += 2048;
        }
    }

    stats
}

fn create_wide_structure(base: &PathBuf, dirs: usize, files_per_dir: usize) -> TestStats {
    let mut stats = TestStats {
        dirs: 0,
        files: 0,
        bytes: 0,
    };

    let content = "x".repeat(100);

    for i in 0..dirs {
        let dir = base.join(format!("dir-{:04}", i));
        fs::create_dir_all(&dir).unwrap();
        stats.dirs += 1;

        for j in 0..files_per_dir {
            let file = dir.join(format!("file-{:04}.txt", j));
            fs::write(&file, &content).unwrap();
            stats.files += 1;
            stats.bytes += content.len() as u64;
        }
    }

    stats
}

fn create_deep_structure(base: &PathBuf, depth: usize, files_per_level: usize) -> TestStats {
    let mut stats = TestStats {
        dirs: 0,
        files: 0,
        bytes: 0,
    };

    let content = "deep content";
    let mut current = base.clone();

    for i in 0..depth {
        current = current.join(format!("level-{}", i));
        fs::create_dir_all(&current).unwrap();
        stats.dirs += 1;

        for j in 0..files_per_level {
            let file = current.join(format!("file-{}.txt", j));
            fs::write(&file, content).unwrap();
            stats.files += 1;
            stats.bytes += content.len() as u64;
        }
    }

    stats
}

fn create_mixed_structure(base: &PathBuf, scale: usize) -> TestStats {
    let mut stats = TestStats {
        dirs: 0,
        files: 0,
        bytes: 0,
    };

    let small_content = "small";
    let medium_content = "x".repeat(1024);
    let large_content = "y".repeat(100 * 1024);

    for i in 0..scale {
        let subdir = base.join(format!("mixed-{}", i));
        fs::create_dir_all(&subdir).unwrap();
        stats.dirs += 1;

        for j in 0..20 {
            let small = subdir.join(format!("small-{}.txt", j));
            fs::write(&small, small_content).unwrap();
            stats.files += 1;
            stats.bytes += small_content.len() as u64;
        }

        for j in 0..5 {
            let medium = subdir.join(format!("medium-{}.dat", j));
            fs::write(&medium, &medium_content).unwrap();
            stats.files += 1;
            stats.bytes += medium_content.len() as u64;
        }

        if i % 5 == 0 {
            let large = subdir.join("large.bin");
            fs::write(&large, &large_content).unwrap();
            stats.files += 1;
            stats.bytes += large_content.len() as u64;
        }

        if i % 3 == 0 {
            let nested = subdir.join("nested");
            fs::create_dir_all(&nested).unwrap();
            stats.dirs += 1;

            for k in 0..10 {
                let file = nested.join(format!("nested-{}.txt", k));
                fs::write(&file, small_content).unwrap();
                stats.files += 1;
                stats.bytes += small_content.len() as u64;
            }
        }
    }

    stats
}

fn run_deletion_test(test_dir: &PathBuf, stats: &TestStats, test_name: &str) -> f64 {
    let start = Instant::now();

    let output = Command::new(rmx_path())
        .args(["-rf", "--stats"])
        .arg(test_dir)
        .output()
        .expect("Failed to execute rmx");

    let elapsed = start.elapsed();

    assert!(
        output.status.success(),
        "{} failed: {}",
        test_name,
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        !test_dir.exists(),
        "{}: directory should be deleted",
        test_name
    );

    let total_items = stats.dirs + stats.files;
    let throughput = total_items as f64 / elapsed.as_secs_f64();

    println!("\n=== {} ===", test_name);
    println!("  Directories: {}", stats.dirs);
    println!("  Files:       {}", stats.files);
    println!("  Total items: {}", total_items);
    println!(
        "  Total size:  {:.2} MB",
        stats.bytes as f64 / 1024.0 / 1024.0
    );
    println!("  Time:        {:.2?}", elapsed);
    println!("  Throughput:  {:.0} items/sec", throughput);

    throughput
}

#[test]
fn stress_test_node_modules_small() {
    let test_dir = create_stress_test_dir("node_modules_small");
    let stats = create_node_modules_structure(&test_dir, 20, 2);

    let throughput = run_deletion_test(&test_dir, &stats, "Node Modules (Small)");
    assert!(
        throughput > 500.0,
        "Throughput {:.0} items/sec is below minimum threshold of 500",
        throughput
    );
}

#[test]
fn stress_test_node_modules_medium() {
    let test_dir = create_stress_test_dir("node_modules_medium");
    let stats = create_node_modules_structure(&test_dir, 50, 3);

    let throughput = run_deletion_test(&test_dir, &stats, "Node Modules (Medium)");
    assert!(
        throughput > 500.0,
        "Throughput {:.0} items/sec is below minimum threshold of 500",
        throughput
    );
}

#[test]
fn stress_test_target_small() {
    let test_dir = create_stress_test_dir("target_small");
    let stats = create_target_structure(&test_dir, 50);

    let throughput = run_deletion_test(&test_dir, &stats, "Rust Target (Small)");
    assert!(
        throughput > 500.0,
        "Throughput {:.0} items/sec is below minimum threshold of 500",
        throughput
    );
}

#[test]
fn stress_test_target_medium() {
    let test_dir = create_stress_test_dir("target_medium");
    let stats = create_target_structure(&test_dir, 200);

    let throughput = run_deletion_test(&test_dir, &stats, "Rust Target (Medium)");
    assert!(
        throughput > 500.0,
        "Throughput {:.0} items/sec is below minimum threshold of 500",
        throughput
    );
}

#[test]
fn stress_test_wide_directories() {
    let test_dir = create_stress_test_dir("wide");
    let stats = create_wide_structure(&test_dir, 500, 20);

    let throughput = run_deletion_test(&test_dir, &stats, "Wide Structure (500 dirs x 20 files)");
    assert!(
        throughput > 1000.0,
        "Throughput {:.0} items/sec is below minimum threshold of 1000",
        throughput
    );
}

#[test]
fn stress_test_deep_nesting() {
    let test_dir = create_stress_test_dir("deep");
    let stats = create_deep_structure(&test_dir, 100, 10);

    let throughput = run_deletion_test(&test_dir, &stats, "Deep Nesting (100 levels)");
    assert!(
        throughput > 500.0,
        "Throughput {:.0} items/sec is below minimum threshold of 500",
        throughput
    );
}

#[test]
fn stress_test_mixed_workload() {
    let test_dir = create_stress_test_dir("mixed");
    let stats = create_mixed_structure(&test_dir, 100);

    let throughput = run_deletion_test(&test_dir, &stats, "Mixed Workload");
    assert!(
        throughput > 500.0,
        "Throughput {:.0} items/sec is below minimum threshold of 500",
        throughput
    );
}

#[test]
fn stress_test_many_small_files() {
    let test_dir = create_stress_test_dir("small_files");
    let mut stats = TestStats {
        dirs: 0,
        files: 0,
        bytes: 0,
    };

    let content = "x";
    for i in 0..100 {
        let subdir = test_dir.join(format!("batch-{}", i));
        fs::create_dir_all(&subdir).unwrap();
        stats.dirs += 1;

        for j in 0..100 {
            let file = subdir.join(format!("tiny-{}.txt", j));
            fs::write(&file, content).unwrap();
            stats.files += 1;
            stats.bytes += 1;
        }
    }

    let throughput = run_deletion_test(&test_dir, &stats, "Many Small Files (10,000 files)");
    assert!(
        throughput > 1000.0,
        "Throughput {:.0} items/sec is below minimum threshold of 1000",
        throughput
    );
}

#[test]
#[ignore]
fn stress_test_large_scale() {
    let test_dir = create_stress_test_dir("large_scale");
    let mut stats = TestStats {
        dirs: 0,
        files: 0,
        bytes: 0,
    };

    println!("Creating large scale test structure...");
    let content = "x".repeat(1024);

    for i in 0..1000 {
        let dir = test_dir.join(format!("dir-{:04}", i));
        fs::create_dir_all(&dir).unwrap();
        stats.dirs += 1;

        for j in 0..50 {
            let file = dir.join(format!("file-{:04}.txt", j));
            fs::write(&file, &content).unwrap();
            stats.files += 1;
            stats.bytes += content.len() as u64;
        }

        if i % 100 == 0 {
            println!("  Created {} directories...", i);
        }
    }

    println!("Structure created. Running deletion test...");
    let throughput = run_deletion_test(&test_dir, &stats, "Large Scale (50,000 files)");
    assert!(
        throughput > 2000.0,
        "Throughput {:.0} items/sec is below minimum threshold of 2000",
        throughput
    );
}
