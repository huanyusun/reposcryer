# Parser Design

RepoScryer uses a registry-driven parser interface:

```rust
pub trait LanguageParser {
    fn parse(&self, file: &CodeFile, source: &str) -> anyhow::Result<ParsedFile>;
}
```

Each supported language has a lightweight parser implementation that extracts:

- imports
- top-level declarations
- selected member declarations such as Rust `impl` methods or Python class/function definitions

The current implementation favors predictable heuristics over deep syntax trees. This keeps Phase 1 small and testable across multiple languages while leaving room for future tree-sitter or compiler-backed parsing.

Parser errors are converted into `IndexWarning` values so a single bad file does not fail the whole indexing run.

Rust `mod name;` declarations are recorded both as module symbols and raw imports. Phase 3 uses those raw imports, plus local `use` paths, to derive conservative file-level dependency edges in Kuzu.
