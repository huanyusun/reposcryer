use anyhow::{Result, anyhow};
use regex::Regex;
use reposcryer_core::{
    Chunk, CodeChunk, CodeFile, FileId, Import, IndexWarning, Language, ParsedFile, Symbol,
    SymbolKind, chunk_id_from_parts, import_id_from_parts, symbol_id_from_parts,
    warning_id_from_parts,
};

pub const PARSER_VERSION: &str = "parser-heuristic-v2";
pub const CHUNKER_VERSION: &str = "symbol-range-v1";

pub trait LanguageParser {
    fn parse(&self, file: &CodeFile, source: &str) -> Result<ParsedFile>;
}

#[derive(Debug, Default)]
pub struct ParserRegistry;

impl ParserRegistry {
    pub fn parse_file(&self, file: &CodeFile, source: &str) -> Result<ParsedFile> {
        match file.language {
            Language::Rust => RustParser.parse(file, source),
            Language::Python => PythonParser.parse(file, source),
            Language::JavaScript | Language::TypeScript => JavaScriptParser.parse(file, source),
            Language::Java => JavaParser.parse(file, source),
            Language::Go => GoParser.parse(file, source),
            Language::Unknown => Ok(empty_parsed_file(&file.file_id)),
        }
    }

    pub fn parse_or_warn(&self, file: &CodeFile, source: &str) -> ParsedFile {
        match self.parse_file(file, source) {
            Ok(parsed) => parsed,
            Err(error) => {
                let mut parsed = empty_parsed_file(&file.file_id);
                parsed.warnings.push(IndexWarning {
                    warning_id: warning_id_from_parts(
                        &file.relative_path,
                        "parse",
                        &error.to_string(),
                    ),
                    file_id: Some(file.file_id.clone()),
                    file_path: file.relative_path.clone(),
                    stage: "parse".to_string(),
                    message: error.to_string(),
                });
                parsed
            }
        }
    }
}

struct RustParser;
struct PythonParser;
struct JavaScriptParser;
struct JavaParser;
struct GoParser;

impl LanguageParser for RustParser {
    fn parse(&self, file: &CodeFile, source: &str) -> Result<ParsedFile> {
        if source.contains("<<parse-error>>") {
            return Err(anyhow!("forced rust parser failure"));
        }

        let mut symbols = Vec::new();
        let mut imports = Vec::new();
        let struct_re = Regex::new(r"^\s*(?:pub\s+)?struct\s+([A-Za-z_][A-Za-z0-9_]*)")?;
        let enum_re = Regex::new(r"^\s*(?:pub\s+)?enum\s+([A-Za-z_][A-Za-z0-9_]*)")?;
        let trait_re = Regex::new(r"^\s*(?:pub\s+)?trait\s+([A-Za-z_][A-Za-z0-9_]*)")?;
        let fn_re = Regex::new(r"^\s*(?:pub\s+)?fn\s+([A-Za-z_][A-Za-z0-9_]*)")?;
        let impl_re = Regex::new(r"^\s*impl(?:\s+[^ ]+)?\s+([A-Za-z_][A-Za-z0-9_]*)")?;
        let use_re = Regex::new(r"^\s*use\s+([^;]+);")?;
        let mod_re = Regex::new(r"^\s*mod\s+([A-Za-z_][A-Za-z0-9_]*)\s*;")?;
        let mut in_impl = false;
        let mut impl_depth: i32 = 0;

        for (idx, line) in source.lines().enumerate() {
            let line_no = idx + 1;
            let brace_delta = line.chars().fold(0_i32, |acc, ch| match ch {
                '{' => acc + 1,
                '}' => acc - 1,
                _ => acc,
            });
            if let Some(caps) = use_re.captures(line) {
                imports.push(Import {
                    import_id: import_id_from_parts(&file.file_id, caps[1].trim(), line_no),
                    from_file_id: file.file_id.clone(),
                    raw_target: caps[1].trim().to_string(),
                    line: line_no,
                });
            }
            if let Some(caps) = mod_re.captures(line) {
                imports.push(Import {
                    import_id: import_id_from_parts(&file.file_id, caps[1].trim(), line_no),
                    from_file_id: file.file_id.clone(),
                    raw_target: caps[1].trim().to_string(),
                    line: line_no,
                });
                symbols.push(make_symbol(file, &caps[1], SymbolKind::Module, line_no));
            }
            if let Some(caps) = struct_re.captures(line) {
                symbols.push(make_symbol(file, &caps[1], SymbolKind::Struct, line_no));
            }
            if let Some(caps) = enum_re.captures(line) {
                symbols.push(make_symbol(file, &caps[1], SymbolKind::Enum, line_no));
            }
            if let Some(caps) = trait_re.captures(line) {
                symbols.push(make_symbol(file, &caps[1], SymbolKind::Trait, line_no));
            }
            if let Some(caps) = impl_re.captures(line) {
                in_impl = true;
                impl_depth += brace_delta;
                symbols.push(make_symbol(file, &caps[1], SymbolKind::Type, line_no));
                continue;
            }
            if let Some(caps) = fn_re.captures(line) {
                let kind = if in_impl {
                    SymbolKind::Method
                } else {
                    SymbolKind::Function
                };
                symbols.push(make_symbol(file, &caps[1], kind, line_no));
            }
            if in_impl {
                impl_depth += brace_delta;
                if impl_depth <= 0 {
                    impl_depth = 0;
                    in_impl = false;
                }
            }
        }

        Ok(parsed_with_chunks(file, symbols, imports, Vec::new()))
    }
}

impl LanguageParser for PythonParser {
    fn parse(&self, file: &CodeFile, source: &str) -> Result<ParsedFile> {
        if source.contains("<<parse-error>>") {
            return Err(anyhow!("forced python parser failure"));
        }

        let mut symbols = Vec::new();
        let mut imports = Vec::new();
        let class_re = Regex::new(r"^\s*class\s+([A-Za-z_][A-Za-z0-9_]*)")?;
        let def_re = Regex::new(r"^\s*def\s+([A-Za-z_][A-Za-z0-9_]*)")?;
        let import_re = Regex::new(r"^\s*import\s+(.+)$")?;
        let from_re = Regex::new(r"^\s*from\s+(.+)\s+import\s+(.+)$")?;

        for (idx, line) in source.lines().enumerate() {
            let line_no = idx + 1;
            if let Some(caps) = import_re.captures(line) {
                imports.push(Import {
                    import_id: import_id_from_parts(&file.file_id, caps[1].trim(), line_no),
                    from_file_id: file.file_id.clone(),
                    raw_target: caps[1].trim().to_string(),
                    line: line_no,
                });
            }
            if let Some(caps) = from_re.captures(line) {
                let target = format!("{}::{}", caps[1].trim(), caps[2].trim());
                imports.push(Import {
                    import_id: import_id_from_parts(&file.file_id, &target, line_no),
                    from_file_id: file.file_id.clone(),
                    raw_target: target,
                    line: line_no,
                });
            }
            if let Some(caps) = class_re.captures(line) {
                symbols.push(make_symbol(file, &caps[1], SymbolKind::Class, line_no));
            }
            if let Some(caps) = def_re.captures(line) {
                symbols.push(make_symbol(file, &caps[1], SymbolKind::Function, line_no));
            }
        }

        Ok(parsed_with_chunks(file, symbols, imports, Vec::new()))
    }
}

impl LanguageParser for JavaScriptParser {
    fn parse(&self, file: &CodeFile, source: &str) -> Result<ParsedFile> {
        if source.contains("<<parse-error>>") {
            return Err(anyhow!("forced js parser failure"));
        }

        let mut symbols = Vec::new();
        let mut imports = Vec::new();
        let import_re = Regex::new(r#"^\s*import\s+.*?from\s+["']([^"']+)["']"#)?;
        let require_re = Regex::new(
            r#"^\s*(?:const|let|var)\s+[A-Za-z_][A-Za-z0-9_]*\s*=\s*require\(["']([^"']+)["']\)"#,
        )?;
        let function_re = Regex::new(r"^\s*(?:export\s+)?function\s+([A-Za-z_][A-Za-z0-9_]*)")?;
        let class_re = Regex::new(r"^\s*(?:export\s+)?class\s+([A-Za-z_][A-Za-z0-9_]*)")?;
        let arrow_re = Regex::new(
            r"^\s*(?:export\s+)?const\s+([A-Za-z_][A-Za-z0-9_]*)\s*=\s*(?:async\s+)?\(",
        )?;

        for (idx, line) in source.lines().enumerate() {
            let line_no = idx + 1;
            if let Some(caps) = import_re.captures(line) {
                imports.push(Import {
                    import_id: import_id_from_parts(&file.file_id, &caps[1], line_no),
                    from_file_id: file.file_id.clone(),
                    raw_target: caps[1].to_string(),
                    line: line_no,
                });
            }
            if let Some(caps) = require_re.captures(line) {
                imports.push(Import {
                    import_id: import_id_from_parts(&file.file_id, &caps[1], line_no),
                    from_file_id: file.file_id.clone(),
                    raw_target: caps[1].to_string(),
                    line: line_no,
                });
            }
            if let Some(caps) = function_re.captures(line) {
                symbols.push(make_symbol(file, &caps[1], SymbolKind::Function, line_no));
            }
            if let Some(caps) = class_re.captures(line) {
                symbols.push(make_symbol(file, &caps[1], SymbolKind::Class, line_no));
            }
            if let Some(caps) = arrow_re.captures(line) {
                symbols.push(make_symbol(file, &caps[1], SymbolKind::Function, line_no));
            }
        }

        Ok(parsed_with_chunks(file, symbols, imports, Vec::new()))
    }
}

impl LanguageParser for JavaParser {
    fn parse(&self, file: &CodeFile, source: &str) -> Result<ParsedFile> {
        let mut symbols = Vec::new();
        let mut imports = Vec::new();
        let import_re = Regex::new(r"^\s*import\s+([^;]+);")?;
        let class_re = Regex::new(r"^\s*(?:public\s+)?class\s+([A-Za-z_][A-Za-z0-9_]*)")?;
        let interface_re = Regex::new(r"^\s*(?:public\s+)?interface\s+([A-Za-z_][A-Za-z0-9_]*)")?;
        let method_re = Regex::new(
            r"^\s*(?:public|private|protected)?\s*(?:static\s+)?[A-Za-z0-9_<>\[\]]+\s+([A-Za-z_][A-Za-z0-9_]*)\s*\(",
        )?;

        for (idx, line) in source.lines().enumerate() {
            let line_no = idx + 1;
            if let Some(caps) = import_re.captures(line) {
                imports.push(Import {
                    import_id: import_id_from_parts(&file.file_id, caps[1].trim(), line_no),
                    from_file_id: file.file_id.clone(),
                    raw_target: caps[1].trim().to_string(),
                    line: line_no,
                });
            }
            if let Some(caps) = class_re.captures(line) {
                symbols.push(make_symbol(file, &caps[1], SymbolKind::Class, line_no));
            }
            if let Some(caps) = interface_re.captures(line) {
                symbols.push(make_symbol(file, &caps[1], SymbolKind::Interface, line_no));
            }
            if let Some(caps) = method_re.captures(line) {
                symbols.push(make_symbol(file, &caps[1], SymbolKind::Method, line_no));
            }
        }

        Ok(parsed_with_chunks(file, symbols, imports, Vec::new()))
    }
}

impl LanguageParser for GoParser {
    fn parse(&self, file: &CodeFile, source: &str) -> Result<ParsedFile> {
        let mut symbols = Vec::new();
        let mut imports = Vec::new();
        let import_re = Regex::new(r#"^\s*"([^"]+)""#)?;
        let func_re = Regex::new(r"^\s*func\s+([A-Za-z_][A-Za-z0-9_]*)\s*\(")?;
        let method_re = Regex::new(r"^\s*func\s+\([^)]+\)\s+([A-Za-z_][A-Za-z0-9_]*)\s*\(")?;
        let type_re = Regex::new(r"^\s*type\s+([A-Za-z_][A-Za-z0-9_]*)\s+(struct|interface)")?;

        for (idx, line) in source.lines().enumerate() {
            let line_no = idx + 1;
            if let Some(caps) = import_re.captures(line) {
                imports.push(Import {
                    import_id: import_id_from_parts(&file.file_id, &caps[1], line_no),
                    from_file_id: file.file_id.clone(),
                    raw_target: caps[1].to_string(),
                    line: line_no,
                });
            }
            if let Some(caps) = method_re.captures(line) {
                symbols.push(make_symbol(file, &caps[1], SymbolKind::Method, line_no));
            } else if let Some(caps) = func_re.captures(line) {
                symbols.push(make_symbol(file, &caps[1], SymbolKind::Function, line_no));
            }
            if let Some(caps) = type_re.captures(line) {
                let kind = if &caps[2] == "struct" {
                    SymbolKind::Struct
                } else {
                    SymbolKind::Interface
                };
                symbols.push(make_symbol(file, &caps[1], kind, line_no));
            }
        }

        Ok(parsed_with_chunks(file, symbols, imports, Vec::new()))
    }
}

fn parsed_with_chunks(
    file: &CodeFile,
    symbols: Vec<Symbol>,
    imports: Vec<Import>,
    warnings: Vec<IndexWarning>,
) -> ParsedFile {
    let chunks = symbols
        .iter()
        .map(|symbol| Chunk {
            chunk_id: chunk_id_from_parts(
                &file.file_id,
                &symbol.name,
                symbol.range.start_line,
                symbol.range.end_line,
            ),
            file_id: file.file_id.clone(),
            symbol_id: Some(symbol.symbol_id.clone()),
            label: format!("symbol:{}", symbol.name),
            range: symbol.range.clone(),
        })
        .collect();

    ParsedFile {
        file_id: file.file_id.clone(),
        symbols,
        imports,
        chunks,
        warnings,
    }
}

fn make_symbol(file: &CodeFile, name: &str, kind: SymbolKind, line_no: usize) -> Symbol {
    Symbol {
        symbol_id: symbol_id_from_parts(&file.relative_path, name, line_no, line_no),
        file_id: file.file_id.clone(),
        file_path: file.relative_path.clone(),
        name: name.to_string(),
        kind,
        range: CodeChunk {
            start_line: line_no,
            end_line: line_no,
        },
        language: file.language.clone(),
    }
}

pub fn empty_parsed_file(file_id: &FileId) -> ParsedFile {
    ParsedFile {
        file_id: file_id.clone(),
        symbols: Vec::new(),
        imports: Vec::new(),
        chunks: Vec::new(),
        warnings: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use reposcryer_core::{FileFingerprint, file_id_from_relative_path, repo_id_from_path};
    use std::path::{Path, PathBuf};

    fn sample_file(path: &str, language: Language) -> CodeFile {
        let relative_path = PathBuf::from(path);
        CodeFile {
            repo_id: repo_id_from_path(Path::new("/tmp/repo")),
            file_id: file_id_from_relative_path(&relative_path),
            path: PathBuf::from(format!("/tmp/repo/{path}")),
            relative_path,
            language,
            fingerprint: FileFingerprint {
                sha256: "abc".to_string(),
                size_bytes: 3,
                mtime_seconds: 1,
            },
        }
    }

    #[test]
    fn rust_parser_extracts_struct_free_function_and_impl_method() {
        let file = sample_file("src/lib.rs", Language::Rust);
        let parsed = ParserRegistry
            .parse_file(
                &file,
                "use crate::db::Database;\nstruct User;\nfn run() {}\nimpl User {\n    fn login(&self) {}\n}\n",
            )
            .expect("parse succeeds");

        assert!(
            parsed
                .symbols
                .iter()
                .any(|symbol| symbol.name == "User" && symbol.kind == SymbolKind::Struct)
        );
        assert!(
            parsed
                .symbols
                .iter()
                .any(|symbol| symbol.name == "run" && symbol.kind == SymbolKind::Function)
        );
        assert!(
            parsed
                .symbols
                .iter()
                .any(|symbol| symbol.name == "login" && symbol.kind == SymbolKind::Method)
        );
        assert!(!parsed.chunks.is_empty());
    }

    #[test]
    fn rust_parser_extracts_mod_declaration_as_import() {
        let file = sample_file("src/main.rs", Language::Rust);
        let parsed = ParserRegistry
            .parse_file(&file, "mod auth;\nuse auth::AuthService;\nfn main() {}\n")
            .expect("parse succeeds");

        assert!(
            parsed
                .imports
                .iter()
                .any(|import| import.raw_target == "auth")
        );
        assert!(
            parsed
                .symbols
                .iter()
                .any(|symbol| symbol.name == "auth" && symbol.kind == SymbolKind::Module)
        );
    }

    #[test]
    fn python_parser_extracts_class_and_def() {
        let file = sample_file("app.py", Language::Python);
        let parsed = ParserRegistry
            .parse_file(
                &file,
                "import os\nclass Service:\n    pass\n\ndef run():\n    pass\n",
            )
            .expect("parse succeeds");

        assert!(parsed.symbols.iter().any(|symbol| symbol.name == "Service"));
        assert!(parsed.symbols.iter().any(|symbol| symbol.name == "run"));
    }

    #[test]
    fn js_ts_parser_extracts_import_and_function_basics() {
        let file = sample_file("app.ts", Language::TypeScript);
        let parsed = ParserRegistry
            .parse_file(
                &file,
                "import { x } from './dep';\nexport function run() {}\nclass Box {}\nconst boot = () => {};\n",
            )
            .expect("parse succeeds");

        assert_eq!(parsed.imports[0].raw_target, "./dep");
        assert!(parsed.symbols.iter().any(|symbol| symbol.name == "run"));
        assert!(parsed.symbols.iter().any(|symbol| symbol.name == "Box"));
        assert!(parsed.symbols.iter().any(|symbol| symbol.name == "boot"));
    }

    #[test]
    fn parser_failure_returns_warning_instead_of_panic() {
        let file = sample_file("broken.rs", Language::Rust);
        let parsed = ParserRegistry.parse_or_warn(&file, "<<parse-error>>");
        assert_eq!(parsed.symbols.len(), 0);
        assert_eq!(parsed.warnings.len(), 1);
    }
}
