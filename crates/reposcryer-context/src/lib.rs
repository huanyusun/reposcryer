use anyhow::Result;
use reposcryer_core::{FileExplanation, FileImpact, FileNeighborhood};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ContextMode {
    Explain,
    ChangePlan,
    Review,
}

impl ContextMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Explain => "explain",
            Self::ChangePlan => "change-plan",
            Self::Review => "review",
        }
    }
}

#[derive(Debug, Clone)]
pub struct ContextInput {
    pub mode: ContextMode,
    pub budget: usize,
    pub explanation: FileExplanation,
    pub neighbors: FileNeighborhood,
    pub impact: FileImpact,
    pub source: String,
    pub repo_map: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ContextPack {
    pub mode: String,
    pub budget: usize,
    pub target_file: PathBuf,
    pub language: String,
    pub symbols: Vec<String>,
    pub imports: Vec<String>,
    pub outgoing_dependencies: Vec<PathBuf>,
    pub incoming_dependencies: Vec<PathBuf>,
    pub impacted_files: Vec<PathBuf>,
    pub warnings: Vec<String>,
    pub source_excerpt: String,
    pub repo_map_excerpt: String,
    pub truncated: bool,
}

pub fn build_context_pack(input: ContextInput) -> Result<ContextPack> {
    let budget = input.budget.max(512);
    let source_budget = budget / 2;
    let repo_map_budget = budget / 4;
    let (source_excerpt, source_truncated) = excerpt(&input.source, source_budget);
    let (repo_map_excerpt, repo_map_truncated) = excerpt(&input.repo_map, repo_map_budget);

    Ok(ContextPack {
        mode: input.mode.as_str().to_string(),
        budget,
        target_file: input.explanation.relative_path.clone(),
        language: input.explanation.language.as_str().to_string(),
        symbols: input
            .explanation
            .symbols
            .iter()
            .map(|symbol| format!("{:?} {}", symbol.kind, symbol.name))
            .collect(),
        imports: input
            .explanation
            .imports
            .iter()
            .map(|import| format!("{}:{}", import.line, import.raw_target))
            .collect(),
        outgoing_dependencies: input
            .neighbors
            .outgoing
            .iter()
            .map(|dependency| dependency.to_relative_path.clone())
            .collect(),
        incoming_dependencies: input
            .neighbors
            .incoming
            .iter()
            .map(|dependency| dependency.from_relative_path.clone())
            .collect(),
        impacted_files: input
            .impact
            .impacted_files
            .iter()
            .map(|file| file.relative_path.clone())
            .collect(),
        warnings: input
            .explanation
            .warnings
            .iter()
            .map(|warning| format!("{}: {}", warning.stage, warning.message))
            .collect(),
        source_excerpt,
        repo_map_excerpt,
        truncated: source_truncated || repo_map_truncated,
    })
}

pub fn render_markdown(pack: &ContextPack) -> String {
    let mut output = String::new();
    output.push_str("# RepoScryer Context Pack\n\n");
    output.push_str(&format!("- mode: {}\n", pack.mode));
    output.push_str(&format!("- target: {}\n", pack.target_file.display()));
    output.push_str(&format!("- language: {}\n", pack.language));
    output.push_str(&format!("- budget: {}\n", pack.budget));
    output.push_str(&format!("- truncated: {}\n\n", pack.truncated));
    push_list(&mut output, "Symbols", &pack.symbols);
    push_list(&mut output, "Imports", &pack.imports);
    push_paths(
        &mut output,
        "Outgoing Dependencies",
        &pack.outgoing_dependencies,
    );
    push_paths(
        &mut output,
        "Incoming Dependencies",
        &pack.incoming_dependencies,
    );
    push_paths(&mut output, "Impacted Files", &pack.impacted_files);
    push_list(&mut output, "Warnings", &pack.warnings);
    output.push_str("## Source Excerpt\n\n```text\n");
    output.push_str(&pack.source_excerpt);
    if !pack.source_excerpt.ends_with('\n') {
        output.push('\n');
    }
    output.push_str("```\n\n## Repo Map Excerpt\n\n```markdown\n");
    output.push_str(&pack.repo_map_excerpt);
    if !pack.repo_map_excerpt.ends_with('\n') {
        output.push('\n');
    }
    output.push_str("```\n");
    output
}

fn push_list(output: &mut String, title: &str, values: &[String]) {
    output.push_str(&format!("## {title}\n\n"));
    if values.is_empty() {
        output.push_str("- none\n\n");
        return;
    }
    for value in values {
        output.push_str(&format!("- {value}\n"));
    }
    output.push('\n');
}

fn push_paths(output: &mut String, title: &str, values: &[PathBuf]) {
    output.push_str(&format!("## {title}\n\n"));
    if values.is_empty() {
        output.push_str("- none\n\n");
        return;
    }
    for value in values {
        output.push_str(&format!("- {}\n", value.display()));
    }
    output.push('\n');
}

fn excerpt(content: &str, budget: usize) -> (String, bool) {
    let mut output = String::new();
    for ch in content.chars().take(budget) {
        output.push(ch);
    }
    let truncated = content.chars().count() > budget;
    if truncated {
        output.push_str("\n...[truncated]\n");
    }
    (output, truncated)
}

#[cfg(test)]
mod tests {
    use super::*;
    use reposcryer_core::{
        CodeChunk, FileExplanation, FileId, FileImpact, FileNeighborhood, ImpactedFile, Import,
        ImportId, Language, Symbol, SymbolId, SymbolKind,
    };
    use std::path::PathBuf;

    fn explanation() -> FileExplanation {
        FileExplanation {
            file_id: FileId("file".to_string()),
            relative_path: PathBuf::from("src/main.rs"),
            language: Language::Rust,
            symbols: vec![Symbol {
                symbol_id: SymbolId("symbol".to_string()),
                file_id: FileId("file".to_string()),
                file_path: PathBuf::from("src/main.rs"),
                name: "main".to_string(),
                kind: SymbolKind::Function,
                range: CodeChunk {
                    start_line: 1,
                    end_line: 1,
                },
                language: Language::Rust,
            }],
            imports: vec![Import {
                import_id: ImportId("import".to_string()),
                from_file_id: FileId("file".to_string()),
                raw_target: "auth".to_string(),
                line: 1,
            }],
            dependencies: Vec::new(),
            warnings: Vec::new(),
        }
    }

    #[test]
    fn context_pack_contains_core_sections_and_truncates_excerpts() {
        let input = ContextInput {
            mode: ContextMode::ChangePlan,
            budget: 512,
            explanation: explanation(),
            neighbors: FileNeighborhood {
                file_id: FileId("file".to_string()),
                relative_path: PathBuf::from("src/main.rs"),
                outgoing: Vec::new(),
                incoming: Vec::new(),
            },
            impact: FileImpact {
                file_id: FileId("file".to_string()),
                relative_path: PathBuf::from("src/main.rs"),
                impacted_files: vec![ImpactedFile {
                    file_id: FileId("impacted".to_string()),
                    relative_path: PathBuf::from("src/app.rs"),
                    depth: 1,
                    via_relative_path: PathBuf::from("src/main.rs"),
                }],
            },
            source: "x".repeat(600),
            repo_map: "# Repo Map\n".to_string(),
        };

        let pack = build_context_pack(input).expect("context pack");
        let markdown = render_markdown(&pack);

        assert_eq!(pack.mode, "change-plan");
        assert_eq!(pack.target_file, PathBuf::from("src/main.rs"));
        assert!(pack.truncated);
        assert!(markdown.contains("# RepoScryer Context Pack"));
        assert!(markdown.contains("## Source Excerpt"));
        assert!(markdown.contains("src/app.rs"));
    }
}
