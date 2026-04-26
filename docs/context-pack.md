# Context Pack

`reposcryer context` builds a deterministic local context bundle for one indexed file.

```bash
reposcryer context <path> --file <file>
reposcryer context <path> --file <file> --json
reposcryer context <path> --file <file> --mode change-plan --budget 4000
```

Modes:

- `explain`: default mode for understanding a file.
- `change-plan`: includes the same deterministic graph context but labels the pack for planning edits.
- `review`: labels the pack for code review workflows.

The current implementation does not call an LLM, does not use embeddings, and does not perform RAG. It composes existing local data:

- Kuzu-backed file explanation
- outgoing and incoming file dependencies
- reverse impact files
- target source excerpt
- repo map excerpt from Phase 1 exports

## Budget

`--budget` is currently an approximate character budget. It controls source and repo map excerpt sizes and marks the pack as `truncated` when either excerpt is shortened. It is not a tokenizer-backed budget.

## Requirements

Run `reposcryer index <path>` first. The target file must be present in the current scan and indexed in Kuzu.
