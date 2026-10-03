use std::fs;
use std::path::Path;
use std::process::Command;

// @kotowari[REQ-016, REQ-advisor-012]
#[test]
fn distributed_single_binary_runs_outside_checkout_and_carries_rust_dependency_license_texts() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let fixture = tempfile::tempdir().unwrap();
    let package = fixture.path().join("package");
    let extracted = fixture.path().join("extracted");
    fs::create_dir(&extracted).unwrap();
    let target = if cfg!(target_env = "musl") {
        "x86_64-unknown-linux-musl"
    } else {
        "x86_64-unknown-linux-gnu"
    };
    let result = Command::new(root.join("scripts/package-release.sh"))
        .arg(env!("CARGO_BIN_EXE_command-guardian"))
        .arg(target)
        .arg(&package)
        .current_dir(root)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "Release archive packaging failed for target {target} in fixture {}: {}\nstdout:\n{}\nstderr:\n{}",
        fixture.path().display(),
        result.status,
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
    let archive = fs::read_dir(&package)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .find(|path| path.extension().is_some_and(|extension| extension == "gz"))
        .unwrap();
    assert!(
        Command::new("tar")
            .arg("-xzf")
            .arg(archive)
            .arg("-C")
            .arg(&extracted)
            .status()
            .unwrap()
            .success()
    );
    let result = Command::new(extracted.join("command-guardian"))
        .arg("--help")
        .current_dir(fixture.path())
        .env_clear()
        .env("HOME", fixture.path())
        .output()
        .unwrap();
    assert!(result.status.success());
    assert!(result.stderr.is_empty());
    assert!(
        String::from_utf8(result.stdout)
            .unwrap()
            .contains("command-guardian check")
    );
    let licenses = extracted.join("licenses/rust");
    let notice = fs::read_to_string(licenses.join("NOTICE.txt")).unwrap();
    assert!(notice.contains("ureq 3.4.2"));
    assert!(notice.contains("ring 0.17.14"));
    assert!(!notice.contains("/home/"));
    for relative in [
        "ureq-3.4.2/LICENSE-MIT",
        "ring-0.17.14/LICENSE-BoringSSL",
        "ring-0.17.14/LICENSE-other-bits",
        "ring-0.17.14/src/polyfill/once_cell/LICENSE-APACHE",
        "webpki-roots-1.0.9/LICENSE",
        "subtle-2.6.1/LICENSE",
    ] {
        assert!(fs::metadata(licenses.join(relative)).unwrap().len() > 100);
    }
}
