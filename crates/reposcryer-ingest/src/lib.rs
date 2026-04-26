use anyhow::{Context, Result};
use ignore::WalkBuilder;
use reposcryer_config::RepoScryerConfig;
use reposcryer_core::{
    CodeFile, FileFingerprint, Language, ScanResult, SkippedFile, file_id_from_project_scope_path,
    project_id_from_repo, repo_id_from_path, scope_id_from_path,
};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

pub fn scan_repo(root: &Path, config: &RepoScryerConfig) -> Result<ScanResult> {
    let repo_root = root
        .canonicalize()
        .with_context(|| format!("failed to canonicalize {}", root.display()))?;
    let repo_id = repo_id_from_path(&repo_root);
    let project_id = project_id_from_repo(&repo_id);
    let scope_id = scope_id_from_path(&repo_root);
    let mut files = Vec::new();
    let mut skipped_files = Vec::new();

    let mut builder = WalkBuilder::new(&repo_root);
    builder
        .hidden(false)
        .git_ignore(true)
        .git_global(true)
        .git_exclude(true);

    for entry in builder.build() {
        let entry = entry?;
        let path = entry.path();

        if path == repo_root {
            continue;
        }

        if !entry
            .file_type()
            .map(|file_type| file_type.is_file())
            .unwrap_or(false)
        {
            continue;
        }

        let relative_path = path.strip_prefix(&repo_root).unwrap_or(path).to_path_buf();
        if should_skip(path, &repo_root, config) {
            continue;
        }

        let metadata = fs::metadata(path)?;
        if metadata.len() > config.max_file_size_bytes {
            skipped_files.push(SkippedFile {
                relative_path,
                reason: "oversized".to_string(),
            });
            continue;
        }

        let bytes = fs::read(path)?;
        if is_binary(&bytes) {
            skipped_files.push(SkippedFile {
                relative_path,
                reason: "binary".to_string(),
            });
            continue;
        }

        let language = detect_language(path);
        if language == Language::Unknown || !config.enabled_languages.contains(&language) {
            continue;
        }

        files.push(CodeFile {
            repo_id: repo_id.clone(),
            file_id: file_id_from_project_scope_path(&project_id, &scope_id, &relative_path),
            path: path.to_path_buf(),
            relative_path,
            language,
            fingerprint: FileFingerprint {
                sha256: sha256_hex(&bytes),
                size_bytes: metadata.len(),
                mtime_seconds: metadata_mtime_seconds(&metadata),
            },
        });
    }

    files.sort_by(|left, right| left.relative_path.cmp(&right.relative_path));
    skipped_files.sort_by(|left, right| left.relative_path.cmp(&right.relative_path));

    Ok(ScanResult {
        repo_id,
        repo_root,
        files,
        skipped_files,
    })
}

pub fn read_source(file: &CodeFile) -> Result<String> {
    fs::read_to_string(&file.path)
        .with_context(|| format!("failed to read source file {}", file.path.display()))
}

fn should_skip(path: &Path, root: &Path, config: &RepoScryerConfig) -> bool {
    let relative: PathBuf = path.strip_prefix(root).unwrap_or(path).to_path_buf();
    relative.components().any(|component| {
        let name = component.as_os_str().to_string_lossy();
        config.ignored_dirs.iter().any(|ignored| ignored == &name)
    })
}

fn is_binary(bytes: &[u8]) -> bool {
    bytes.iter().take(1024).any(|byte| *byte == 0)
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

fn metadata_mtime_seconds(metadata: &fs::Metadata) -> u64 {
    metadata
        .modified()
        .ok()
        .and_then(|mtime| mtime.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_secs())
        .unwrap_or_default()
}

pub fn detect_language(path: &Path) -> Language {
    match path.extension().and_then(|extension| extension.to_str()) {
        Some("rs") => Language::Rust,
        Some("py") => Language::Python,
        Some("js") | Some("jsx") => Language::JavaScript,
        Some("ts") | Some("tsx") => Language::TypeScript,
        Some("java") => Language::Java,
        Some("go") => Language::Go,
        _ => Language::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn scans_sample_project_rust_files() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/sample-rust-project");
        let scan = scan_repo(&root, &RepoScryerConfig::default()).expect("scan succeeds");
        let names: Vec<_> = scan
            .files
            .iter()
            .map(|file| file.relative_path.to_string_lossy().to_string())
            .collect();

        assert!(names.iter().any(|name| name == "src/main.rs"));
        assert!(names.iter().any(|name| name == "src/auth.rs"));
        assert!(names.iter().any(|name| name == "src/db.rs"));
    }

    #[test]
    fn ignores_target_and_node_modules() {
        let dir = tempdir().expect("tempdir");
        let root = dir.path();
        fs::create_dir_all(root.join("src")).expect("src");
        fs::create_dir_all(root.join("target")).expect("target");
        fs::create_dir_all(root.join("node_modules")).expect("node_modules");
        fs::write(root.join("src/lib.rs"), "pub fn run() {}\n").expect("write rust");
        fs::write(root.join("target/ignored.rs"), "pub fn nope() {}\n").expect("write target");
        fs::write(
            root.join("node_modules/ignored.js"),
            "export const nope = 1;\n",
        )
        .expect("write node_modules");

        let scan = scan_repo(root, &RepoScryerConfig::default()).expect("scan succeeds");
        assert_eq!(scan.files.len(), 1);
        assert_eq!(scan.files[0].relative_path, PathBuf::from("src/lib.rs"));
        assert!(scan.skipped_files.is_empty());
    }

    #[test]
    fn detects_rust_language_from_extension() {
        assert_eq!(detect_language(Path::new("demo.rs")), Language::Rust);
    }

    #[test]
    fn computes_sha256() {
        let dir = tempdir().expect("tempdir");
        let root = dir.path();
        fs::create_dir_all(root.join("src")).expect("src");
        fs::write(root.join("src/lib.rs"), "pub fn run() {}\n").expect("write rust");

        let scan = scan_repo(root, &RepoScryerConfig::default()).expect("scan succeeds");
        assert_eq!(scan.files.len(), 1);
        assert_eq!(scan.files[0].fingerprint.sha256.len(), 64);
    }

    #[test]
    fn honors_configured_ignored_dirs_size_limit_and_languages() {
        let dir = tempdir().expect("tempdir");
        let root = dir.path();
        fs::create_dir_all(root.join("src")).expect("src");
        fs::create_dir_all(root.join("generated")).expect("generated");
        fs::write(root.join("src/lib.rs"), "pub fn run() {}\n").expect("write rust");
        fs::write(root.join("src/app.py"), "x=1\n").expect("write python");
        fs::write(root.join("src/large.rs"), "pub fn run_large() {}\n").expect("write large");
        fs::write(root.join("generated/ignored.rs"), "pub fn generated() {}\n")
            .expect("write ignored");

        let config = RepoScryerConfig {
            ignored_dirs: vec!["generated".to_string()],
            max_file_size_bytes: 18,
            enabled_languages: vec![Language::Rust],
            ..RepoScryerConfig::default()
        };

        let scan = scan_repo(root, &config).expect("scan succeeds");
        let names: Vec<_> = scan
            .files
            .iter()
            .map(|file| file.relative_path.to_string_lossy().to_string())
            .collect();

        assert_eq!(names, ["src/lib.rs"]);
        assert_eq!(scan.skipped_files.len(), 1);
        assert_eq!(
            scan.skipped_files[0].relative_path,
            PathBuf::from("src/large.rs")
        );
        assert_eq!(scan.skipped_files[0].reason, "oversized");
    }
}
