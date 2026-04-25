# Contributing

RepoScryer uses a branch-and-PR workflow.

## Branch Policy

- `main` is the protected integration branch.
- Do not commit directly to `main`.
- Create feature branches with a descriptive name, for example `codex/phase3-impact-query`.
- Open a pull request into `main` for every change.
- Merge only after CI passes.

## Required Local Checks

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

## Release Policy

Merges to `main` run CI and then create an automated GitHub release from the merged commit.

The current release workflow builds the `reposcryer-cli` binary for Linux x86_64 and publishes it as a GitHub release artifact. Broader target matrices can be added later.
