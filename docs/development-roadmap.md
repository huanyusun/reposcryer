# Development Roadmap

This document translates the product roadmap into an implementation sequence. Every non-trivial change should be developed test-first and merged through a pull request into `main`.

## Branch and PR Rules

- Start each work item from the latest `main`.
- Use a descriptive branch name, for example `codex/phase3-json-query-api`.
- Keep each PR scoped to one milestone.
- Run local checks before pushing.
- Merge only after CI passes.

## Required Checks

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo run -p reposcryer-cli -- index examples/sample-rust-project
cargo run -p reposcryer-cli -- status examples/sample-rust-project
cargo run -p reposcryer-cli -- changed examples/sample-rust-project
cargo run -p reposcryer-cli -- explain examples/sample-rust-project src/main.rs
cargo run -p reposcryer-cli -- graph neighbors examples/sample-rust-project src/main.rs
cargo run -p reposcryer-cli -- graph summary examples/sample-rust-project
cargo run -p reposcryer-cli -- impact examples/sample-rust-project src/auth.rs
cargo run -p reposcryer-cli -- graph rebuild examples/sample-rust-project
```

## Next PRs

### PR 2: JSON Query API

Branch: `codex/phase3-json-query-api`

Status: in progress.

Scope:

- Add `--json` for `explain`.
- Add `--json` for `graph neighbors`.
- Add `--json` for `impact`.
- Add stable output structs if current core structs need CLI-safe wrappers.
- Add tests for valid JSON and important fields.
- Update README and architecture docs.

Acceptance:

- Human-readable output remains default.
- JSON includes file IDs, relative paths, confidence, and evidence where available.
- No Kuzu query logic is added to `reposcryer-cli`.

### PR 3: Graph Summary

Branch: `codex/phase3-graph-summary`

Status: in progress.

Scope:

- Add `GraphStore::scope_graph_summary`.
- Add `reposcryer graph summary <path>`.
- Report active files, deleted files, symbols, imports, dependency edges, warnings, and latest index run status.
- Add CLI and store tests.

Acceptance:

- Summary is read from Kuzu, not Phase 1 export files.
- Deleted files are counted separately from active files.
- Output is deterministic.

### PR 4: Real Config Loading

Branch: `codex/config-loading`

Status: in progress.

Scope:

- Implement `.reposcryer/config.toml` parsing.
- Add config defaults and validation.
- Add `reposcryer config init <path>`.
- Add tests for missing, valid, and invalid config.

Acceptance:

- Existing default behavior remains unchanged without a config file.
- User config is not overwritten unless explicitly requested.
- Scan behavior honors configured ignored directories and file size limits.

### PR 6: Rust Resolver Fidelity

Branch: `codex/rust-resolver-fidelity`

Status: in progress.

Scope:

- Improve `self::` and `super::` resolution.
- Improve nested module resolution.
- Improve `mod.rs`, `lib.rs`, and `main.rs` behavior.
- Add multi-file Rust fixture tests.

Acceptance:

- Resolver remains conservative and evidence-driven.
- External crates and standard library imports do not become local dependency edges.
- No `CALLS` edges are generated.

### PR 7: Context Pack

Branch: `codex/context-pack`

Scope:

- Add `reposcryer context <path> --file <file>`.
- Generate Markdown output.
- Generate JSON output.
- Include explain, neighbors, impact, symbols, warnings, and repo map excerpts.
- Add `--budget` to limit output size.

Acceptance:

- Context output is deterministic.
- Context does not call an LLM.
- No embeddings or RAG are introduced.

### PR 8: Test Graph and Test Suggestions

Branch: `codex/test-graph`

Scope:

- Detect test files and test symbols with explicit evidence.
- Add Kuzu relationship for test coverage if the evidence is clear.
- Add `reposcryer test-suggest <path> <file>`.
- Add Rust test fixtures.

Acceptance:

- Test suggestions are evidence-based.
- Unsupported or ambiguous cases are omitted rather than guessed.

### PR 9: Diff-Aware Impact

Branch: `codex/diff-aware-impact`

Scope:

- Read git changed files.
- Add `reposcryer impact --changed`.
- Aggregate impact across changed files.
- Include dependency paths and depths.

Acceptance:

- Works without remote GitHub access.
- Handles deleted and renamed files conservatively.

## Release Milestones

- `v0.1`: baseline Kuzu index, incremental indexing, Phase 1 exports, file dependency graph, query commands, CI/release with Linux, macOS, and Windows artifacts.
- `v0.2`: JSON query API and graph summary.
- `v0.3`: real config loading.
- `v0.4`: Rust resolver fidelity.
- `v0.5`: context pack.
- `v0.6`: broader resolver and test graph.
- `v0.7`: diff-aware impact.
- `v0.8`: Skill integration.

## Guardrails

- Keep Kuzu as the single source of truth.
- Keep CLI as orchestration only.
- Do not add separate SQLite metadata.
- Do not add Tantivy until graph/query/context behavior is stable.
- Do not add embeddings or RAG until there is a concrete use case.
- Do not add MCP or Web UI until stable CLI APIs exist.
- Do not fabricate call graph edges.
