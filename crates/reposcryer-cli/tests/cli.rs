use assert_cmd::Command;
use serde_json::Value;
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
fn config_init_creates_default_config_without_overwrite() {
    let dir = tempdir().expect("tempdir");

    Command::cargo_bin("reposcryer-cli")
        .expect("binary")
        .args(["config", "init"])
        .arg(dir.path())
        .assert()
        .success();

    let config_path = dir.path().join(".reposcryer/config.toml");
    assert!(config_path.exists());
    let original = fs::read_to_string(&config_path).expect("read config");
    fs::write(&config_path, "max_file_size_bytes = 64\n").expect("custom config");

    Command::cargo_bin("reposcryer-cli")
        .expect("binary")
        .args(["config", "init"])
        .arg(dir.path())
        .assert()
        .success();

    assert_ne!(original, "max_file_size_bytes = 64\n");
    assert_eq!(
        fs::read_to_string(&config_path).expect("read config"),
        "max_file_size_bytes = 64\n"
    );

    Command::cargo_bin("reposcryer-cli")
        .expect("binary")
        .args(["config", "init", "--force"])
        .arg(dir.path())
        .assert()
        .success();

    assert_eq!(
        fs::read_to_string(&config_path).expect("read config"),
        original
    );
}

#[test]
fn index_honors_dot_reposcryer_config() {
    let dir = tempdir().expect("tempdir");
    fs::create_dir_all(dir.path().join("src")).expect("src");
    fs::create_dir_all(dir.path().join("generated")).expect("generated");
    fs::create_dir_all(dir.path().join(".reposcryer")).expect("config dir");
    fs::write(dir.path().join("src/lib.rs"), "pub fn run() {}\n").expect("rust");
    fs::write(dir.path().join("src/app.py"), "x=1\n").expect("python");
    fs::write(
        dir.path().join("generated/ignored.rs"),
        "pub fn ignored() {}\n",
    )
    .expect("ignored");
    fs::write(
        dir.path().join(".reposcryer/config.toml"),
        r#"
max_file_size_bytes = 1024
ignored_dirs = [".git", ".reposcryer", "generated"]
enabled_languages = ["rust"]
"#,
    )
    .expect("config");

    Command::cargo_bin("reposcryer-cli")
        .expect("binary")
        .arg("index")
        .arg(dir.path())
        .assert()
        .success();

    let mut command = Command::cargo_bin("reposcryer-cli").expect("binary");
    let assert = command.arg("status").arg(dir.path()).assert().success();
    let stdout = String::from_utf8(assert.get_output().stdout.clone()).expect("utf8");

    assert!(stdout.contains("tracked_files: 1"));
    assert!(stdout.contains("scan_files: 1"));
    assert!(!stdout.contains("src/app.py"));
    assert!(!stdout.contains("generated/ignored.rs"));
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
fn graph_summary_shows_kuzu_scope_counts() {
    let dir = temp_sample_repo();

    Command::cargo_bin("reposcryer-cli")
        .expect("binary")
        .arg("index")
        .arg(dir.path())
        .assert()
        .success();

    let mut command = Command::cargo_bin("reposcryer-cli").expect("binary");
    let assert = command
        .args(["graph", "summary"])
        .arg(dir.path())
        .assert()
        .success();
    let stdout = String::from_utf8(assert.get_output().stdout.clone()).expect("utf8");

    assert!(stdout.contains("active_files: 3"));
    assert!(stdout.contains("deleted_files: 0"));
    assert!(stdout.contains("dependency_edges: 3"));
    assert!(stdout.contains("latest_run_status: completed"));
}

#[test]
fn graph_summary_json_outputs_structured_counts() {
    let dir = temp_sample_repo();

    Command::cargo_bin("reposcryer-cli")
        .expect("binary")
        .arg("index")
        .arg(dir.path())
        .assert()
        .success();

    let mut command = Command::cargo_bin("reposcryer-cli").expect("binary");
    let assert = command
        .args(["graph", "summary"])
        .arg(dir.path())
        .arg("--json")
        .assert()
        .success();
    let stdout = String::from_utf8(assert.get_output().stdout.clone()).expect("utf8");
    let value: Value = serde_json::from_str(&stdout).expect("valid json");

    assert_eq!(value["active_files"], 3);
    assert_eq!(value["deleted_files"], 0);
    assert_eq!(value["dependency_edges"], 3);
    assert_eq!(value["latest_run_status"], "completed");
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

#[test]
fn explain_json_outputs_structured_file_context() {
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
        .arg("--json")
        .assert()
        .success();
    let stdout = String::from_utf8(assert.get_output().stdout.clone()).expect("utf8");
    let value: Value = serde_json::from_str(&stdout).expect("valid json");

    assert_eq!(value["relative_path"], "src/main.rs");
    assert_eq!(value["language"], "rust");
    assert!(
        value["dependencies"]
            .as_array()
            .expect("dependencies array")
            .iter()
            .any(|dependency| dependency["to_relative_path"] == "src/auth.rs")
    );
}

#[test]
fn graph_neighbors_json_outputs_incoming_and_outgoing() {
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
        .arg("--json")
        .assert()
        .success();
    let stdout = String::from_utf8(assert.get_output().stdout.clone()).expect("utf8");
    let value: Value = serde_json::from_str(&stdout).expect("valid json");

    assert_eq!(value["relative_path"], "src/main.rs");
    assert!(
        value["outgoing"]
            .as_array()
            .expect("outgoing array")
            .iter()
            .any(|dependency| dependency["to_relative_path"] == "src/db.rs")
    );
}

#[test]
fn impact_json_outputs_structured_impacted_files() {
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
        .arg("--json")
        .assert()
        .success();
    let stdout = String::from_utf8(assert.get_output().stdout.clone()).expect("utf8");
    let value: Value = serde_json::from_str(&stdout).expect("valid json");

    assert_eq!(value["relative_path"], "src/auth.rs");
    assert!(
        value["impacted_files"]
            .as_array()
            .expect("impacted files array")
            .iter()
            .any(|file| file["relative_path"] == "src/main.rs" && file["depth"] == 1)
    );
}
