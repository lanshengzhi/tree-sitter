---
title: "Canonical S-expression Form v1"
type: contract
status: active
date: 2026-04-26
origin: docs/plans/2026-04-26-001-feat-r0-context-firewall-plan.md
---

# Canonical S-expression Form v1

## Scope

This document defines the canonical S-expression serialization format for the R0 v1 `tree-sitter-context` bridge. It governs:

- Rust CLI output (`tree-sitter-context bundle`)
- pi-mono parser/canonicalizer input and re-emission
- Golden test byte-equality assertions

All v1 S-expression bytes must be deterministic: repeated serialization of the same logical value produces identical byte sequences.

## Grammar

A canonical S-expression is an **atom** or a **list**.

```text
expr     := atom | list
atom     := string | symbol | integer
list     := '(' [ ws symbol [ ws expr ]* ]? ws? ')'
symbol   := [a-zA-Z_][a-zA-Z0-9_-]*
string   := '"' [char]* '"'
char     := unescaped | escape
unescaped:= [^\"\\\x00-\x1f\x7f]
escape   := '\\' ( '"' | '\\' | 'n' | 't' )
integer  := [0-9]+
ws       := [ \t\n]+
```

Rules:
- Lists are space-separated, not comma-separated.
- Symbols are lowercase with hyphens and underscores.
- No comments.
- No bare integers as node types; integers appear only as field values.

## String Escaping

The canonical subset escapes exactly these characters:

| Character | Escape Sequence |
|-----------|----------------|
| `"`       | `\"`           |
| `\`       | `\\`           |
| newline   | `\n`           |
| tab       | `\t`           |

All other printable ASCII characters (0x20-0x7E) appear unescaped. Control characters outside the escape subset are rejected as invalid during serialization.

## Indentation

- Two spaces per nesting level.
- First child of a list appears on the same line after the opening symbol, unless the list exceeds 80 columns.
- For readability in this document, multi-line forms are shown indented; the canonical bytes must preserve exact indentation.

Example:
```text
(bundle
  (version 1)
  (path "src/lib.rs")
  (cells
    (cell
      (stable_id "named:abc123...")
      (kind "function_item")
      (name "foo")
      (range 0 23)
      (estimated_tokens 6)
      (confidence exact))))
```

## Node Ordering

Fields within a record must appear in the order defined by the protocol schema. Lists of records must be sorted by a deterministic key:

- `cells` and `omitted` lists sort by `stable_id` ascending.
- `candidates` lists sort by `anchor_byte` ascending, then `stable_id`.
- Parameter and ref lists sort by `name`, then `stable_id`.

## Negative Result Forms

Negative results are valid S-expressions returned on stdout with exit code 0.

### not_found

```text
(not_found
  (path "src/lib.rs")
  (stable_id "named:foo")
  (reason "no chunk with this stable_id found in file")
  (provenance
    (strategy "stable_id_lookup")
    (confidence 0)
    (graph_snapshot_id "<XXH3 hex>")
    (orientation_freshness fresh)))
```

### ambiguous_stable_id

```text
(ambiguous_stable_id
  (path "src/lib.rs")
  (stable_id "named:foo")
  (candidates
    (candidate
      (anchor_byte 0)
      (kind "function_item")
      (name "foo"))
    (candidate
      (anchor_byte 45)
      (kind "function_item")
      (name "foo")))
  (reason "multiple chunks share this stable_id")
  (provenance
    (strategy "stable_id_lookup")
    (confidence 0)
    (graph_snapshot_id "<XXH3 hex>")
    (orientation_freshness stale)))
```

### exhausted

```text
(exhausted
  (path "src/lib.rs")
  (stable_id "named:foo")
  (omitted
    (omitted_chunk
      (stable_id "named:foo")
      (reason "over_budget")))
  (provenance
    (strategy "sig_tier_bundle")
    (confidence exact)
    (graph_snapshot_id "<XXH3 hex>")
    (orientation_freshness unknown)))
```

### unknown_cross_file

```text
(unknown_cross_file
  (path "src/lib.rs")
  (stable_id "named:foo")
  (reason "v1-non-goal"))
```

## Provenance Block

Every result (positive or negative) includes a provenance block:

```text
(provenance
  (strategy <symbol>)
  (confidence <symbol-or-integer>)
  (graph_snapshot_id <string>)
  (orientation_freshness <string>))
```

- `strategy`: symbolic name of the resolution strategy.
- `confidence`: `exact`, `high`, `medium`, `low`, or integer 0-100.
- `graph_snapshot_id`: current HEAD snapshot ID (`"<XXH3 hex>"`), or `"no_graph"` when no graph has been built.
- `orientation_freshness`: `fresh`, `stale`, or `unknown` (enum locked in R0).

## Error Forms (Process Failures)

Process-level failures (missing file, unreadable path, missing language) use non-zero exit codes and stderr. They do not appear as stdout S-expressions.

## Version Marker

Every positive bundle includes a top-level version marker:

```text
(version 1)
```

This enables future format evolution without breaking pi-mono parser assumptions.

## Reserved Values

The following are reserved for future versions but must not be produced or consumed in v1:

- `tier` values other than `"sig"`.
- `output_format` values other than `"sexpr"`.
- `graph_snapshot_id` values that are not valid XXH3 hex IDs or `"no_graph"`.
- `orientation_freshness` values other than `fresh`, `stale`, or `unknown`.

## Determinism Guarantees

1. Same logical value → same bytes.
2. Same file + same stable_id + same tier + same budget → same bytes.
3. pi-mono parse + canonical re-emit must be byte-equal to Rust output for valid input.

## Rejection Rules

A pi-mono parser must reject (and map to `rust-output-invalid`) any Rust stdout that:
- Contains characters outside the grammar above.
- Has mismatched parentheses.
- Uses unescaped control characters in strings.
- Places integers where symbols are required (e.g., node type names).
- Omits required fields in a record.

## Compatibility

This format is scoped to the pi-mono R0 bridge. The existing `tree-sitter context` JSON output remains unchanged and is not governed by this document.
