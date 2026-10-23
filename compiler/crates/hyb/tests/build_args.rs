//! `build`, `explain`, `vendor-runtime`, and `bench-overhead` must validate
//! their CLI arguments before reading sources, probing toolchains, or writing
//! outputs. Mirrors the fail-closed CLI contracts in `check_args.rs` and
//! `generate_paths.rs`.

use std::fs;
use std::path::PathBuf;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT: AtomicUsize = AtomicUsize::new(0);

struct Workspace(PathBuf);

impl Workspace {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "hyb-build-args-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).expect("create unique workspace directory");
        fs::write(root.join("good.hy"), "fn main() {}\n").unwrap();
        fs::write(root.join("bad.hy"), "fn main( {\n").unwrap();
        Self(root)
    }

    fn hyb(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_hyb"))
            .args(args)
            .env("HYB_ALLOW_TOOLCHAIN_DRIFT", "1")
            .current_dir(&self.0)
            .output()
            .expect("run hyb CLI")
    }
}

impl Drop for Workspace {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn assert_usage_error(output: Output, expected: &str) {
    assert_eq!(
        output.status.code(),
        Some(2),
        "expected usage error (exit 2), stdout={}, stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        output.stdout.is_empty(),
        "no partial stdout on usage error: {}",
        String::from_utf8_lossy(&output.stdout)
    );
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(
        stderr.contains(expected),
        "missing {expected:?} in stderr: {stderr}"
    );
    assert!(
        !stderr.contains("error reading"),
        "must validate arguments before file I/O: {stderr}"
    );
}

#[test]
fn build_rejects_unknown_options_and_extra_files_before_io() {
    let ws = Workspace::new();
    for args in [
        vec!["build", "--unknown", "good.hy", "--emit-go"],
        vec!["build", "good.hy", "--emit-go", "--unknown"],
        vec!["build", "good.hy", "--emit-go=true"],
        vec!["build", "-x", "good.hy"],
        vec!["build", "missing.hy", "--unknown"],
    ] {
        assert_usage_error(ws.hyb(&args), "unknown build option");
        assert!(!ws.0.join("a.out.go").exists());
    }

    for args in [
        vec!["build", "good.hy", "bad.hy", "--emit-go"],
        vec!["build", "missing.hy", "good.hy", "--emit-go"],
        vec!["build", "--emit-go", "--", "good.hy", "--emit-go"],
    ] {
        assert_usage_error(ws.hyb(&args), "exactly one input file");
        assert!(!ws.0.join("a.out.go").exists());
    }
}

#[test]
fn build_rejects_missing_option_values_and_repeated_flags() {
    let ws = Workspace::new();
    for (args, expected) in [
        (vec!["build", "good.hy", "-o"], "-o requires a value"),
        (
            vec!["build", "good.hy", "-o", "--emit-go"],
            "-o requires a value, not an option",
        ),
        (
            vec!["build", "good.hy", "-o", ""],
            "-o requires a value, not an option",
        ),
        (
            vec!["build", "good.hy", "--package"],
            "--package requires a value",
        ),
        (
            vec!["build", "good.hy", "--out-dir"],
            "--out-dir requires a value",
        ),
        (
            vec!["build", "good.hy", "--binding-prefix"],
            "--binding-prefix requires a value",
        ),
        (
            vec!["build", "good.hy", "-o", "first", "-o", "second", "--emit-go"],
            "-o may only be supplied once",
        ),
        (
            vec![
                "build",
                "good.hy",
                "--package",
                "a",
                "--package",
                "b",
                "--out-dir",
                "pkg",
            ],
            "--package may only be supplied once",
        ),
    ] {
        assert_usage_error(ws.hyb(&args), expected);
        assert!(!ws.0.join("a.out.go").exists());
        assert!(!ws.0.join("--emit-go.go").exists());
    }
}

#[test]
fn build_rejects_incomplete_or_conflicting_package_mode_flags() {
    let ws = Workspace::new();
    for (args, expected) in [
        (
            vec!["build", "good.hy", "--package", "demo"],
            "--package and --out-dir must be supplied together",
        ),
        (
            vec!["build", "good.hy", "--out-dir", "out"],
            "--package and --out-dir must be supplied together",
        ),
        (
            vec![
                "build",
                "good.hy",
                "--package",
                "demo",
                "--out-dir",
                "out",
                "-o",
                "bin",
            ],
            "--package mode cannot be combined with -o or --emit-go",
        ),
        (
            vec![
                "build",
                "good.hy",
                "--package",
                "demo",
                "--out-dir",
                "out",
                "--emit-go",
            ],
            "--package mode cannot be combined with -o or --emit-go",
        ),
        (
            vec![
                "build",
                "good.hy",
                "--binding-prefix",
                "example.com/p",
                "--emit-go",
            ],
            "--binding-prefix requires --package and --out-dir",
        ),
    ] {
        assert_usage_error(ws.hyb(&args), expected);
        assert!(!ws.0.join("a.out.go").exists());
    }
}

#[test]
fn build_output_path_ending_in_hy_does_not_masquerade_as_input_source() {
    let ws = Workspace::new();
    // Previously `args.iter().find(|a| a.ends_with(".hy"))` picked `-o`'s
    // value `out.hy` as the input source file instead of `good.hy`.
    let output = ws.hyb(&["build", "-o", "out.hy", "good.hy", "--emit-go"]);
    assert!(
        output.status.success(),
        "stdout={}, stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(ws.0.join("out.hy.go").exists());
}

#[test]
fn explain_vendor_runtime_and_bench_overhead_reject_unexpected_args() {
    let ws = Workspace::new();
    assert_usage_error(
        ws.hyb(&["explain", "--json", "HYB-E1001"]),
        "unknown option",
    );
    assert_usage_error(
        ws.hyb(&["explain", "HYB-E1001", "HYB-E1002"]),
        "exactly one diagnostic code",
    );

    assert_usage_error(
        ws.hyb(&["vendor-runtime", "--check", "target-dir"]),
        "unknown vendor-runtime option",
    );
    assert_usage_error(
        ws.hyb(&["vendor-runtime", "dir1", "dir2"]),
        "exactly one target directory",
    );
    assert!(!ws.0.join("dir1").exists());

    assert_usage_error(
        ws.hyb(&["bench-overhead", "--unknown"]),
        "bench-overhead takes no arguments",
    );
}
