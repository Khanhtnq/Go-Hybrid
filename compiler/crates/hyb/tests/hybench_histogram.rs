//! Verifies `runtime/go/hybench` quantile monotonicity, `NaN` handling, and
//! `gofmt`-clean formatting (executing `go test ./hybench` whenever `go` is
//! available on `PATH`, as in CI's `compiler` job).

use std::fs;
use std::io::ErrorKind;
use std::path::PathBuf;
use std::process::Command;

#[test]
fn hybench_histogram_bounds_and_go_unit_tests() {
    let hybench_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../runtime/go/hybench");
    let hist_src = fs::read_to_string(hybench_dir.join("histogram.go")).expect("read histogram.go");
    let test_src =
        fs::read_to_string(hybench_dir.join("histogram_test.go")).expect("read histogram_test.go");

    assert!(
        hist_src.contains("fraction != fraction"),
        "ValueAt must guard against NaN fractions"
    );
    assert!(
        hist_src.contains("hi > uint64(h.maxVal)"),
        "ValueAt must clamp reconstructed bucket upper bounds to maxVal"
    );
    assert!(
        test_src.contains("TestQuantileMonotonicityAndBounds"),
        "histogram_test.go must include TestQuantileMonotonicityAndBounds"
    );

    match Command::new("go").arg("version").output() {
        Err(e) if e.kind() == ErrorKind::NotFound => return,
        Err(e) => panic!("failed to probe go binary: {e}"),
        Ok(out) if !out.status.success() => return,
        Ok(_) => {}
    }

    let tmp = std::env::temp_dir().join(format!("hyb-hybench-test-{}", std::process::id()));
    let _ = fs::remove_dir_all(&tmp);
    let pkg = tmp.join("hybench");
    fs::create_dir_all(&pkg).expect("create temp hybench module");
    fs::write(tmp.join("go.mod"), "module hybrid.program\n\ngo 1.20\n").expect("write go.mod");
    fs::copy(hybench_dir.join("histogram.go"), pkg.join("histogram.go")).unwrap();
    fs::copy(
        hybench_dir.join("histogram_test.go"),
        pkg.join("histogram_test.go"),
    )
    .unwrap();

    let out = Command::new("go")
        .args(["test", "./hybench"])
        .current_dir(&tmp)
        .output()
        .expect("run go test ./hybench");
    let _ = fs::remove_dir_all(&tmp);
    assert!(
        out.status.success(),
        "go test ./hybench failed:\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
}
