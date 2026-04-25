# RepoScryer Agent Instructions

## Project Mission
RepoScryer is a Rust-first local repo intelligence engine for AI coding agents.
Its goal is to provide reliable, low-token, queryable, incrementally updatable codebase context through CLI + Skill integration.

## Current Phase
Current implementation phase: Phase 3.
Implement reliable graph enhancement on top of the Phase 2 Kuzu store.

## Hard Boundaries
Allowed in this phase:
- Kuzu store schema extensions
- GraphStore query extensions
- reliable file-level import / dependency graph
- Rust module/import resolution to local files when evidence is explicit
- idempotent rebuild of derived file dependency edges
- explain command for stored file context
- file-level graph neighbors query
- file-level impact analysis based only on reverse `IMPORTS_FILE` traversal
- documentation and tests for graph enhancement limitations

Still forbidden in this phase:
- RAG implementation
- workspace multi-project management
- branch-aware or worktree-aware indexing
- Web UI
- MCP
- embedding provider
- separate SQLite metadata database
- separate Tantivy search index
- LSP integration
- fabricated or speculative call graph edges

## Architecture Rules
- Keep logic out of reposcryer-cli when possible. CLI should orchestrate crate APIs.
- Do not create empty future crates just to mirror the roadmap.
- Core data models belong in reposcryer-core.
- File scanning belongs in reposcryer-ingest.
- Parsing belongs in reposcryer-parser.
- Graph construction belongs in reposcryer-graph.
- Export logic belongs in reposcryer-export.
- Configuration belongs in reposcryer-config.

## Correctness Rules
- Do not claim call graph accuracy in Phase 3.
- Do not fabricate Symbol calls Symbol edges.
- Do not generate CALLS edges without reliable parser evidence and tests.
- File dependency edges must record confidence and evidence.
- Every GraphEdge must include confidence and evidence.
- Parser failures must become warnings, not panics.
- If a parser is heuristic, document the limitation.

## TDD Workflow
All non-trivial behavior must be developed test-first.

Required loop:

1. Write or update a failing test that defines the expected behavior.
2. Implement the minimal code to pass the test.
3. Refactor without changing behavior.
4. Run the relevant crate tests.
5. Only then move to the next behavior.

TDD applies especially to:
- file scanning and ignore rules
- language detection
- stable ID generation
- parser extraction
- graph edge construction
- import resolution
- file dependency edge construction
- graph query APIs
- file-level impact traversal
- Kuzu schema initialization
- file fingerprint persistence
- incremental plan generation
- file-level subgraph replacement
- soft delete behavior
- index run recording
- export file generation
- CLI index happy path
- CLI status / changed / graph rebuild behavior
- warning behavior

Do not add behavior without tests unless it is trivial wiring.

## Quality Gates
Before considering work complete, run:

```
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo run -p reposcryer-cli -- index examples/sample-rust-project
cargo run -p reposcryer-cli -- status examples/sample-rust-project
cargo run -p reposcryer-cli -- changed examples/sample-rust-project
cargo run -p reposcryer-cli -- explain examples/sample-rust-project src/main.rs
cargo run -p reposcryer-cli -- graph rebuild examples/sample-rust-project
```

## Documentation Rules
When adding or changing behavior, update the relevant docs:
- README.md
- docs/architecture.md
- docs/kuzu-store.md
- docs/parser-design.md
- docs/limitations.md
- docs/roadmap.md

## Decision Policy
If unsure, choose the simpler maintainable design and document the limitation.
Do not interrupt the user for minor implementation choices.
