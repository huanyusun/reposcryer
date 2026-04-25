use anyhow::Result;
use reposcryer_config::RepoScryerConfig;
use reposcryer_core::RepoIndex;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct ExportPaths {
    pub output_dir: PathBuf,
    pub graph_json: PathBuf,
    pub symbols_json: PathBuf,
    pub repo_map_md: PathBuf,
    pub warnings_json: PathBuf,
}

pub fn export_repo_index(
    root: &Path,
    config: &RepoScryerConfig,
    index: &RepoIndex,
) -> Result<ExportPaths> {
    let output_dir = root.join(&config.output_dir).join("exports");
    fs::create_dir_all(&output_dir)?;

    let graph_json = output_dir.join("graph.json");
    let symbols_json = output_dir.join("symbols.json");
    let repo_map_md = output_dir.join("repo-map.md");
    let warnings_json = output_dir.join("warnings.json");

    fs::write(&graph_json, serde_json::to_string_pretty(&index.edges)?)?;
    fs::write(&symbols_json, serde_json::to_string_pretty(&index.symbols)?)?;
    fs::write(
        &warnings_json,
        serde_json::to_string_pretty(&index.warnings)?,
    )?;
    fs::write(&repo_map_md, render_repo_map(index))?;

    Ok(ExportPaths {
        output_dir,
        graph_json,
        symbols_json,
        repo_map_md,
        warnings_json,
    })
}

pub fn load_graph(path: &Path) -> Result<String> {
    Ok(fs::read_to_string(
        path.join(".reposcryer/exports/graph.json"),
    )?)
}

pub fn load_symbols(path: &Path) -> Result<String> {
    Ok(fs::read_to_string(
        path.join(".reposcryer/exports/symbols.json"),
    )?)
}

pub fn load_warnings(path: &Path) -> Result<String> {
    Ok(fs::read_to_string(
        path.join(".reposcryer/exports/warnings.json"),
    )?)
}

pub fn load_repo_map(path: &Path) -> Result<String> {
    Ok(fs::read_to_string(
        path.join(".reposcryer/exports/repo-map.md"),
    )?)
}

fn render_repo_map(index: &RepoIndex) -> String {
    let mut language_counts: BTreeMap<String, usize> = BTreeMap::new();
    let mut directory_counts: BTreeMap<String, usize> = BTreeMap::new();
    let mut import_counts: BTreeMap<String, usize> = BTreeMap::new();

    for file in &index.files {
        *language_counts
            .entry(file.language.as_str().to_string())
            .or_default() += 1;
        let directory = file
            .relative_path
            .parent()
            .map(|path| path.to_string_lossy().to_string())
            .filter(|path| !path.is_empty())
            .unwrap_or_else(|| ".".to_string());
        *directory_counts.entry(directory).or_default() += 1;
    }

    for edge in &index.edges {
        if edge.kind == reposcryer_core::EdgeKind::Imports {
            *import_counts
                .entry(edge.evidence.detail.clone())
                .or_default() += 1;
        }
    }

    let language_summary = language_counts
        .into_iter()
        .map(|(language, count)| format!("- {language}: {count}"))
        .collect::<Vec<_>>()
        .join("\n");
    let directory_summary = directory_counts
        .into_iter()
        .map(|(directory, count)| format!("- {directory}: {count}"))
        .collect::<Vec<_>>()
        .join("\n");
    let symbol_summary = index
        .symbols
        .iter()
        .map(|symbol| {
            format!(
                "- {} ({:?}) in {}",
                symbol.name,
                symbol.kind,
                symbol.file_path.display()
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let imports_summary = import_counts
        .into_iter()
        .map(|(detail, count)| format!("- {detail}: {count}"))
        .collect::<Vec<_>>()
        .join("\n");
    let warning_summary = if index.warnings.is_empty() {
        "- none".to_string()
    } else {
        index
            .warnings
            .iter()
            .map(|warning| {
                format!(
                    "- {} [{}] {}",
                    warning.file_path.display(),
                    warning.stage,
                    warning.message
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    };

    format!(
        "# Repo Map\n\n- Repo path: {}\n- Indexed at: {}\n- File count: {}\n\n## Language summary\n{}\n\n## Directory summary\n{}\n\n## Symbol summary\n{}\n\n## Imports summary\n{}\n\n## Warnings\n{}\n",
        index.repo_root.display(),
        index.indexed_at,
        index.files.len(),
        language_summary,
        directory_summary,
        symbol_summary,
        if imports_summary.is_empty() {
            "- none"
        } else {
            &imports_summary
        },
        warning_summary
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use reposcryer_core::{
        CodeChunk, CodeFile, Confidence, EdgeId, EdgeKind, Evidence, FileFingerprint, GraphEdge,
        Language, RepoId, RepoIndex, Symbol, SymbolId, SymbolKind,
    };
    use std::path::PathBuf;
    use tempfile::tempdir;

    fn sample_index() -> RepoIndex {
        RepoIndex {
            repo_id: RepoId("repo".to_string()),
            repo_root: PathBuf::from("/tmp/repo"),
            indexed_at: "1".to_string(),
            files: vec![CodeFile {
                repo_id: RepoId("repo".to_string()),
                file_id: reposcryer_core::FileId("file".to_string()),
                path: PathBuf::from("/tmp/repo/src/main.rs"),
                relative_path: PathBuf::from("src/main.rs"),
                language: Language::Rust,
                fingerprint: FileFingerprint {
                    sha256: "abc".to_string(),
                    size_bytes: 3,
                    mtime_seconds: 1,
                },
            }],
            symbols: vec![Symbol {
                symbol_id: SymbolId("sym".to_string()),
                file_id: reposcryer_core::FileId("file".to_string()),
                file_path: PathBuf::from("src/main.rs"),
                name: "run".to_string(),
                kind: SymbolKind::Function,
                range: CodeChunk {
                    start_line: 1,
                    end_line: 1,
                },
                language: Language::Rust,
            }],
            edges: vec![GraphEdge {
                edge_id: EdgeId("edge".to_string()),
                from_id: "file".to_string(),
                to_id: "sym".to_string(),
                kind: EdgeKind::Defines,
                confidence: Confidence {
                    score: 1.0,
                    label: "high".to_string(),
                },
                evidence: Evidence {
                    source: "test".to_string(),
                    detail: "defines:run".to_string(),
                },
            }],
            warnings: Vec::new(),
        }
    }

    #[test]
    fn export_generates_required_files() {
        let dir = tempdir().expect("tempdir");
        let index = sample_index();
        let paths =
            export_repo_index(dir.path(), &RepoScryerConfig::default(), &index).expect("export");
        assert!(paths.graph_json.exists());
        assert!(paths.symbols_json.exists());
        assert!(paths.repo_map_md.exists());
        assert!(paths.warnings_json.exists());
    }

    #[test]
    fn repo_map_contains_file_count_and_symbol_summary() {
        let index = sample_index();
        let content = render_repo_map(&index);
        assert!(content.contains("File count: 1"));
        assert!(content.contains("run"));
    }
}
