# Roadmap

RepoScryer is a Rust-first local repo intelligence engine for AI coding agents. The long-term direction is to provide reliable, low-token, queryable, incrementally updated codebase context through a CLI-first workflow and later Skill integration.

The roadmap is intentionally staged. Kuzu graph correctness, incremental indexing, and agent-friendly query APIs come before RAG, MCP, Web UI, or workspace-scale features.

## v0.1: Baseline Graph Index

Status: current PR baseline.

- Rust workspace skeleton and crate boundaries.
- Repo scan with language detection, ignore handling, and file fingerprints.
- Heuristic parsers for Rust, Python, JavaScript, TypeScript, Java, and Go.
- Phase 1 exports: `graph.json`, `symbols.json`, `repo-map.md`, `warnings.json`.
- Kuzu as the single source of truth for graph data and incremental state.
- `GraphStore` trait and `KuzuGraphStore`.
- Schema initialization and schema version tracking.
- `IndexRun` tracking.
- File-level incremental plan generation.
- Idempotent file subgraph replacement.
- Soft delete for deleted files.
- CLI: `index`, `status`, `changed`, `graph rebuild`.
- Phase 3 starter graph queries: `explain`, `graph neighbors`, `impact`.
- CI and automated release workflow with Linux, macOS, and Windows packages.

## v0.2: Agent-Friendly Query API

Goal: make stored graph context directly consumable by agents and scripts.

- Add `--json` output for `explain`, `graph neighbors`, and `impact`.
- Add `reposcryer graph summary <path>`.
- Add stable JSON schemas for query outputs.
- Include confidence and evidence in JSON query responses.
- Add tests that validate JSON output shape.
- Keep CLI human-readable output as the default.

## v0.3: Real Configuration

Goal: make indexing behavior configurable without code changes.

- Implement real `.reposcryer/config.toml` loading.
- Add `reposcryer config init <path>`.
- Support configured ignored directories.
- Support configured max file size.
- Support enabled language filters.
- Persist config defaults safely without overwriting user edits.
- Document config schema in `docs/configuration.md`.

## v0.4: Rust Resolver Fidelity

Goal: make Rust file dependency edges more reliable while staying file-level.

- Support `self::` and `super::` module paths.
- Improve nested module resolution with deepest-prefix matching.
- Improve `mod.rs`, `lib.rs`, and `main.rs` handling.
- Detect and ignore standard library imports explicitly.
- Add richer Rust resolver fixtures.
- Keep `IMPORTS_FILE` evidence explicit and conservative.

## v0.5: Context Pack

Goal: generate low-token context bundles for AI coding agents.

- Add `reposcryer context <path> --file <file>`. Current branch: in progress.
- Support Markdown and JSON output.
- Include target file explanation, nearby dependencies, reverse impact, symbols, warnings, and repo map excerpts.
- Add approximate budget controls such as `--budget 4000`.
- Add context modes such as `explain`, `change-plan`, and `review`.
- Do not add embeddings or RAG in this phase.

## v0.6: Broader Resolver and Test Graph

Goal: broaden reliable graph coverage beyond the initial Rust module graph.

- Add local Python import resolution.
- Add local JavaScript and TypeScript import resolution.
- Add Go and Java local import resolution where evidence is clear.
- Detect test files and test symbols.
- Add file-level `TESTS_FILE` or equivalent relationship only when evidence is explicit.
- Add `reposcryer test-suggest <path> <file>` based on file dependency and test graph evidence.

## v0.7: Diff-Aware Impact Intelligence

Goal: support code review and change planning workflows.

- Add `reposcryer impact --changed`.
- Use git changed files as input to impact traversal.
- Output dependency paths, depth, confidence, and evidence.
- Suggest likely affected files and tests.
- Add context generation for changed files.
- Keep analysis local and deterministic.

## v0.8: Skill Integration

Goal: make RepoScryer easy for coding agents to use consistently.

- Generate a RepoScryer Skill package.
- Add `reposcryer skill export`.
- Document when agents should run `index`, `context`, `impact`, and `changed`.
- Keep Skill integration file-based and CLI-based.
- Do not introduce MCP yet.

## v0.9: Optional Search Layer

Goal: add text search only if graph/path/symbol queries are insufficient.

- Start with Kuzu-backed path and symbol queries.
- Evaluate Tantivy only after query APIs and context packs are stable.
- If added, Tantivy must remain a derived search index, not a source of truth.
- Embedding providers and RAG remain out of scope unless a concrete use case justifies them.

## v1.0: Workspace and Integration Readiness

Goal: stabilize the project for broader usage.

- Harden Kuzu concurrency and lock behavior.
- Add migration strategy beyond rebuild-only schema changes.
- Harden packaged binary release coverage beyond the initial Linux, macOS, and Windows matrix if needed.
- Add workspace and multi-project management only after single-repo behavior is stable.
- Evaluate MCP adapter and Web UI as wrappers around existing stable APIs.

## Non-Goals Until Explicitly Approved

- No fabricated call graph.
- No `CALLS` edges without reliable parser evidence and tests.
- No RAG implementation.
- No embedding provider.
- No MCP adapter.
- No Web UI.
- No separate SQLite metadata database.
- No separate Tantivy search index before graph/query/context layers are stable.
