use assert_cmd::Command;
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::tempdir;

fn sample_project_fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/sample-rust-project")
}

fn copy_dir_all(src: &Path, dst: &Path) {
    fs::create_dir_all(dst).expect("create dst");
    for entry in fs::read_dir(src).expect("read dir") {
        let entry = entry.expect("entry");
        let ty = entry.file_type().expect("file type");
        let dst_path = dst.join(entry.file_name());
        if ty.is_dir() {
            copy_dir_all(&entry.path(), &dst_path);
        } else {
            fs::copy(entry.path(), dst_path).expect("copy file");
        }
    }
}

fn temp_sample_repo() -> tempfile::TempDir {
    let dir = tempdir().expect("tempdir");
    copy_dir_all(&sample_project_fixture(), dir.path());
    dir
}

#[test]
fn index_defaults_to_incremental() {
    let dir = temp_sample_repo();
    let mut command = Command::cargo_bin("reposcryer-cli").expect("binary");
    command.arg("index").arg(dir.path());
    command.assert().success();

    assert!(dir.path().join(".reposcryer/exports/graph.json").exists());
    assert!(dir.path().join(".reposcryer/exports/symbols.json").exists());
    assert!(dir.path().join(".reposcryer/exports/repo-map.md").exists());
    assert!(
        dir.path()
            .join(".reposcryer/exports/warnings.json")
            .exists()
    );
    assert!(dir.path().join(".reposcryer/kuzu").exists());
}

#[test]
fn status_shows_added_modified_deleted_unchanged() {
    let dir = temp_sample_repo();

    Command::cargo_bin("reposcryer-cli")
        .expect("binary")
        .arg("index")
        .arg(dir.path())
        .assert()
        .success();

    fs::write(
        dir.path().join("src/main.rs"),
        "mod auth;\nmod db;\n\nfn main() {\n    println!(\"changed\");\n}\n",
    )
    .expect("modify");
    fs::remove_file(dir.path().join("src/auth.rs")).expect("delete");
    fs::write(dir.path().join("src/new.rs"), "pub fn added() {}\n").expect("add");

    let mut command = Command::cargo_bin("reposcryer-cli").expect("binary");
    let assert = command.arg("status").arg(dir.path()).assert().success();
    let stdout = String::from_utf8(assert.get_output().stdout.clone()).expect("utf8");

    assert!(stdout.contains("added: 1"));
    assert!(stdout.contains("modified: 1"));
    assert!(stdout.contains("deleted: 1"));
    assert!(stdout.contains("unchanged: 1"));
}

#[test]
fn changed_lists_changed_files() {
    let dir = temp_sample_repo();

    Command::cargo_bin("reposcryer-cli")
        .expect("binary")
        .arg("index")
        .arg(dir.path())
        .assert()
        .success();

    fs::write(dir.path().join("src/db.rs"), "pub fn connect() {}\n").expect("modify");

    let mut command = Command::cargo_bin("reposcryer-cli").expect("binary");
    let assert = command.arg("changed").arg(dir.path()).assert().success();
    let stdout = String::from_utf8(assert.get_output().stdout.clone()).expect("utf8");

    assert!(stdout.contains("modified\tsrc/db.rs"));
}

#[test]
fn graph_rebuild_recreates_kuzu_database() {
    let dir = temp_sample_repo();

    Command::cargo_bin("reposcryer-cli")
        .expect("binary")
        .arg("index")
        .arg(dir.path())
        .assert()
        .success();

    let marker = dir.path().join(".reposcryer/kuzu/marker.txt");
    fs::write(&marker, "stale").expect("marker");
    assert!(marker.exists());

    Command::cargo_bin("reposcryer-cli")
        .expect("binary")
        .args(["graph", "rebuild"])
        .arg(dir.path())
        .assert()
        .success();

    assert!(dir.path().join(".reposcryer/kuzu").exists());
    assert!(!marker.exists());
}

#[test]
fn explain_shows_resolved_file_dependencies() {
    let dir = temp_sample_repo();

    Command::cargo_bin("reposcryer-cli")
        .expect("binary")
        .arg("index")
        .arg(dir.path())
        .assert()
        .success();

    let mut command = Command::cargo_bin("reposcryer-cli").expect("binary");
    let assert = command
        .arg("explain")
        .arg(dir.path())
        .arg("src/main.rs")
        .assert()
        .success();
    let stdout = String::from_utf8(assert.get_output().stdout.clone()).expect("utf8");

    assert!(stdout.contains("file: src/main.rs"));
    assert!(stdout.contains("imports_file: src/auth.rs"));
    assert!(stdout.contains("imports_file: src/db.rs"));
}

#[test]
fn graph_neighbors_shows_incoming_and_outgoing_file_dependencies() {
    let dir = temp_sample_repo();

    Command::cargo_bin("reposcryer-cli")
        .expect("binary")
        .arg("index")
        .arg(dir.path())
        .assert()
        .success();

    let mut command = Command::cargo_bin("reposcryer-cli").expect("binary");
    let assert = command
        .args(["graph", "neighbors"])
        .arg(dir.path())
        .arg("src/main.rs")
        .assert()
        .success();
    let stdout = String::from_utf8(assert.get_output().stdout.clone()).expect("utf8");

    assert!(stdout.contains("file: src/main.rs"));
    assert!(stdout.contains("outgoing: src/auth.rs"));
    assert!(stdout.contains("outgoing: src/db.rs"));
}

#[test]
fn impact_shows_reverse_file_dependencies() {
    let dir = temp_sample_repo();

    Command::cargo_bin("reposcryer-cli")
        .expect("binary")
        .arg("index")
        .arg(dir.path())
        .assert()
        .success();

    let mut command = Command::cargo_bin("reposcryer-cli").expect("binary");
    let assert = command
        .arg("impact")
        .arg(dir.path())
        .arg("src/auth.rs")
        .assert()
        .success();
    let stdout = String::from_utf8(assert.get_output().stdout.clone()).expect("utf8");

    assert!(stdout.contains("file: src/auth.rs"));
    assert!(stdout.contains("impacted: depth=1 src/main.rs"));
}
