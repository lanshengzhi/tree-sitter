# tree-sitter-context

> **Status: Experimental Prototype** — Schema version `0.1.0`. Public contract may change.

`tree-sitter-context` is a low-level code context engine built on tree-sitter. It turns source code into small, accurate, incrementally-updatable semantic units for LLM coding agents and code intelligence tools.

It is **not** a complete agent, a code review product, or an MCP server. It is a reusable primitive: chunk files at AST boundaries, extract symbols, compute incremental invalidation, and pack context into token budgets.

## Table of Contents

- [Why](#why)
- [Core Concepts](#core-concepts)
- [CLI Quick Start](#cli-quick-start)
- [JSON Output Schema](#json-output-schema)
  - [`ContextOutput` — chunks and symbols](#contextoutput--chunks-and-symbols)
  - [`InvalidationOutput` — old/new snapshot diff](#invalidationoutput--oldnew-snapshot-diff)
  - [`BundleOutput` — budgeted context](#bundleoutput--budgeted-context)
- [Library API](#library-api)
- [Confidence and Diagnostics](#confidence-and-diagnostics)
- [Current Limitations](#current-limitations)

---

## Why

Coding agents today waste context in three ways:

1. **Reading entire files** — imports, comments, and boilerplate dilute model attention.
2. **Broad `grep` results** — keyword matches return large, unscoped source blocks.
3. **Re-reading unchanged code** — after an edit, agents lack incremental semantic invalidation.

`tree-sitter-context` addresses all three by providing:

- **Syntax-aware chunking** — boundaries at functions, structs, impls, etc.
- **Stable chunk identity** — IDs survive byte-offset shifts and minor edits.
- **Semantic invalidation** — old/new snapshot diff maps `Tree::changed_ranges` to affected chunks.
- **Token budgeting** — pack the most relevant chunks under a limit, with explicit omissions.
- **Symbol metadata** — definitions, references, docs via `tree-sitter-tags` queries.

### How this differs from existing tools

| Tool | Layer | What it does |
|------|-------|-------------|
| Aider repo map | Product | File and symbol summaries for LLM context |
| Graphify | Product | Tree-sitter + graph retrieval knowledge base |
| Code Review Graph | Product | MCP code graph for review context |
| **tree-sitter-context** | **Engine** | **Reusable semantic context primitives for the above** |

If you are building an agent, editor plugin, or code tool, this crate gives you the building blocks without locking you into a specific product or protocol.

---

## Core Concepts

### Chunk

A **chunk** is a syntactically meaningful unit of code: a function, struct, enum, impl block, module, etc. Each chunk carries:

- `id` — run-local identifier (path, kind, name, anchor byte)
- `stable_id` — cross-run hash identity (survives shifts and edits)
- `byte_range` — source byte range
- `estimated_tokens` — conservative token estimate (bytes ÷ 4)
- `confidence` — `exact`, `high`, `medium`, or `low`
- `depth` / `parent` — nesting information

### Stable Identity

Named chunks are identified by `(path, kind, name, parent)`. Unnamed chunks fall back to `(path, kind, content_hash, parent)`. This lets you cache chunk metadata across parse runs and detect renames, moves, and edits.

### Invalidation

Given an old and new snapshot of the same file, the engine:

1. Parses both and extracts chunks.
2. Matches chunks by `stable_id` to find unchanged, added, and removed.
3. Detects changed ranges (via tree-sitter incremental parse + textual diff).
4. Classifies overlapping chunks as `affected`.
5. Reports detailed `InvalidationRecord`s with reason and confidence.

### Budgeted Bundle

Given a token budget, the engine greedily packs chunks (top-level first, smaller first) and returns:

- `included` — chunks that fit
- `omitted` — chunks that did not fit, with reason (`over_budget`, `low_priority`)
- `diagnostics` — what was left out and why

---

## CLI Quick Start

The `context` command is integrated into the main `tree-sitter` CLI (alias: `ctx`).

```bash
# Build and run via cargo
cargo run -p tree-sitter-cli -- context src/lib.rs

# Or after installing tree-sitter CLI
tree-sitter context src/lib.rs
```

### Commands

#### Extract chunks

```bash
tree-sitter context src/lib.rs
```

Outputs [`ContextOutput`](#contextoutput--chunks-and-symbols) JSON with chunks, symbols, diagnostics, and metadata.

#### Include symbols

```bash
tree-sitter context src/lib.rs --symbols
```

Attaches definitions and references via `tags.scm` / `locals.scm` queries.

#### Old/new snapshot invalidation

```bash
tree-sitter context src/lib.rs --old /path/to/old_version.rs
```

Outputs [`InvalidationOutput`](#invalidationoutput--oldnew-snapshot-diff) JSON classifying chunks as affected, added, removed, or unchanged.

#### Budgeted bundle

```bash
tree-sitter context src/lib.rs --budget 2048
```

Outputs [`BundleOutput`](#bundleoutput--budgeted-context) JSON with included and omitted chunks.

> **Note:** `--symbols` is incompatible with `--budget` in the current CLI. If both are requested, symbols are omitted with a diagnostic warning.

### CLI Options

```
Usage: tree-sitter context [OPTIONS] <PATH>

Arguments:
  <PATH>  The source file to extract context from

Options:
      --old <OLD>           Old snapshot for invalidation diff
      --symbols             Include symbols (definitions/references)
      --budget <TOKENS>     Maximum token budget for the context bundle
  -p, --grammar-path <...>  Path to grammar directory
      --config-path <...>   Alternative config.json
  -r, --rebuild             Force rebuild parser
  -q, --quiet               Suppress main output
  -h, --help                Print help
```

---

## JSON Output Schema

All outputs share a common diagnostic and metadata structure.

### `ContextOutput` — chunks and symbols

Emitted by `tree-sitter context <file>` and `tree-sitter context <file> --symbols`.

```json
{
  "chunks": [
    {
      "id": {
        "path": "src/lib.rs",
        "kind": "function_item",
        "name": "parse_query",
        "anchor_byte": 120
      },
      "stable_id": "named:a1b2c3...",
      "kind": "function_item",
      "name": "parse_query",
      "byte_range": { "start": 120, "end": 450 },
      "estimated_tokens": 82,
      "confidence": "exact",
      "depth": 0
    }
  ],
  "symbols": [
    {
      "name": "parse_query",
      "syntax_type": "function",
      "byte_range": { "start": 120, "end": 450 },
      "lines": { "start": 10, "end": 25 },
      "docs": "Parse a tree-sitter query from source.",
      "is_definition": true,
      "path": "src/lib.rs",
      "confidence": "exact"
    }
  ],
  "diagnostics": [],
  "meta": {
    "schema_version": "0.1.0",
    "source_path": "src/lib.rs",
    "total_chunks": 1,
    "total_estimated_tokens": 82
  }
}
```

**Key fields:**

| Field | Meaning |
|-------|---------|
| `chunks[].id` | Run-local identifier (not stable across edits) |
| `chunks[].stable_id` | Cross-run identity string (`named:` or `unnamed:` prefix) |
| `chunks[].estimated_tokens` | Conservative estimate: `ceil(byte_len / 4)` |
| `chunks[].confidence` | `exact` (no parse errors), `low` (parse errors present) |
| `symbols[].is_definition` | `true` for definitions, `false` for references |
| `symbols[].docs` | Leading doc comments if available via tags query |
| `diagnostics` | Structured warnings/errors about limits, missing queries, parse errors |

### `InvalidationOutput` — old/new snapshot diff

Emitted by `tree-sitter context <file> --old <old_file>`.

```json
{
  "records": [
    {
      "status": "affected",
      "chunk": { /* ChunkRecord */ },
      "old_chunk": { /* ChunkRecord */ },
      "reason": "changed_range_overlap",
      "match_strategy": "stable_id",
      "confidence": "exact",
      "changed_ranges": [{ "start": 200, "end": 250 }]
    }
  ],
  "affected": [ /* ChunkRecords */ ],
  "added": [],
  "removed": [],
  "unchanged": [ /* ChunkRecords */ ],
  "changed_ranges": [{ "start": 200, "end": 250 }],
  "diagnostics": [],
  "meta": { /* OutputMeta */ }
}
```

**Statuses:**

| Status | Meaning |
|--------|---------|
| `affected` | Chunk overlaps a changed range or its content changed |
| `added` | Present in new snapshot, absent in old |
| `removed` | Present in old snapshot, absent in new |
| `unchanged` | Stable identity matched and no overlap with changed ranges |

**Reasons:**

| Reason | Meaning |
|--------|---------|
| `changed_range_overlap` | Chunk byte range overlaps a tree-sitter changed range |
| `content_changed` | Stable identity matched but content differs (e.g. literal edit) |
| `added_chunk` | New chunk with no stable match in old snapshot |
| `removed_chunk` | Old chunk with no stable match in new snapshot |
| `no_change_detected` | Identity matched and content unchanged |
| `degraded_matching` | Fallback matching used (low confidence) |

**Match strategies:**

| Strategy | Meaning |
|----------|---------|
| `stable_id` | Matched by `stable_id` hash |
| `content_comparison` | Matched by content hash |
| `textual_range_overlap` | Matched by byte range overlap |
| `edit_range_overlap` | Matched by edit range overlap |
| `unmatched` | No match found |

### `BundleOutput` — budgeted context

Emitted by `tree-sitter context <file> --budget <TOKENS>`.

```json
{
  "included": [ /* ChunkRecords that fit */ ],
  "omitted": [
    {
      "chunk": { /* ChunkRecord */ },
      "reason": "over_budget"
    }
  ],
  "total_included_tokens": 1800,
  "total_omitted_tokens": 3200,
  "budget": 2048,
  "diagnostics": [
    {
      "level": "info",
      "code": "general_info",
      "message": "3 chunk(s) omitted due to budget (1800/2048 tokens used)"
    }
  ]
}
```

**Bundling algorithm:**

1. Sort chunks by `depth` ascending (top-level first), then `estimated_tokens` ascending (smaller first).
2. Include chunks until `max_tokens` or `max_chunks` is reached.
3. Remaining chunks go to `omitted` with reason.

---

## Library API

You can also use `tree-sitter-context` as a Rust library.

```rust
use tree_sitter::Parser;
use tree_sitter_context::{
    chunk::{chunks_for_tree, ChunkOptions},
    identity::{StableId, match_chunks},
    invalidation::invalidate_snapshot,
    bundle::{bundle_chunks, BundleOptions},
    symbols::{symbols_for_tree, SymbolOptions},
    schema::{ContextOutput, Diagnostic},
};

// 1. Parse source
let mut parser = Parser::new();
parser.set_language(&language)?;
let tree = parser.parse(source, None).unwrap();

// 2. Extract chunks
let chunk_result = chunks_for_tree(&tree, path, source, &ChunkOptions::default());

// 3. Extract symbols (requires TagsConfiguration)
let symbol_result = symbols_for_tree(path, source, tags_config, &SymbolOptions::default());

// 4. Invalidate across snapshots
let invalidation = invalidate_snapshot(&old_tree, &new_tree, old_source, new_source, path)?;

// 5. Bundle under budget
let bundle = bundle_chunks(chunk_result.chunks, &BundleOptions { max_tokens: 2048, ..Default::default() });
```

### Re-exported types

The crate re-exports all public types from submodules:

```rust
pub use tree_sitter_context::{
    BundleOptions, BundleOutput, OmissionReason, bundle_chunks,
    ChunkOptions, ChunkOutput, chunks_for_tree,
    StableId, match_chunks,
    invalidate_edits, invalidate_snapshot,
    ByteRange, ChunkId, ChunkRecord, Confidence, ContextOutput, Diagnostic,
    DiagnosticCode, DiagnosticLevel, InvalidationOutput, InvalidationReason,
    InvalidationRecord, InvalidationStatus, MatchStrategy, OutputMeta, SymbolRecord,
};
```

---

## Confidence and Diagnostics

Every output includes `confidence` and `diagnostics` fields. Integrations should treat these as first-class signals, not optional metadata.

### Confidence levels

| Level | When |
|-------|------|
| `exact` | Parse succeeded, no errors, named boundary |
| `high` | Strong heuristic match |
| `medium` | Best-effort with known limitations (default) |
| `low` | Parse errors present, fallback boundaries used |

### Diagnostic fields

```json
{
  "level": "warning",
  "code": "general_warning",
  "message": "parse tree contains syntax errors; chunk confidence downgraded",
  "cause": null,
  "fix": null,
  "context": null,
  "source": "new_snapshot_chunking"
}
```

| Field | Purpose |
|-------|---------|
| `level` | `info`, `warning`, `error` |
| `code` | Machine-readable code for programmatic handling |
| `message` | Human-readable description |
| `cause` | Optional underlying cause |
| `fix` | Optional suggested fix |
| `context` | Optional additional context |
| `source` | Optional subsystem that emitted the diagnostic |

---

## Current Limitations

1. **Schema stability** — Output format is `0.1.0` and may change. Do not hard-code field expectations in production integrations without version gating.
2. **Language coverage** — Chunk boundary heuristics are generic (function, class, struct, enum, trait, impl, module) but work best for C-family languages. Rust is the primary test target.
3. **Cross-file symbols** — Symbol extraction is same-file only in v1. Cross-file resolution requires an external index.
4. **Token estimation** — Uses a conservative byte heuristic (`ceil(bytes / 4)`), not a real tokenizer. Real token counts may differ.
5. **No MCP server** — This is a library/CLI primitive. An MCP adapter is a future possibility, not current scope.
6. **Budget bundling does not include symbols** — `--budget` and `--symbols` are mutually exclusive in the current CLI.
7. **Parse error handling** — Files with syntax errors are chunked at fallback boundaries with `confidence: low`.

---

## Further Reading

- [`docs/plans/tree-sitter-context-rfc-2026-04-24.md`](../../docs/plans/tree-sitter-context-rfc-2026-04-24.md) — Full RFC with background, goals, non-goals, and architecture
- [`docs/plans/tree-sitter-context-hardening-implementation-plan-2026-04-25.md`](../../docs/plans/tree-sitter-context-hardening-implementation-plan-2026-04-25.md) — Implementation plan
- `crates/context/src/` — Source code for all modules
