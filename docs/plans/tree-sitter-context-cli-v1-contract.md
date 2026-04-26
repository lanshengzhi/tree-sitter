---
title: "tree-sitter-context CLI v1 Contract"
type: contract
status: active
date: 2026-04-26
origin: docs/plans/2026-04-26-001-feat-r0-context-firewall-plan.md
---

# tree-sitter-context CLI v1 Contract

## Command

```text
tree-sitter-context bundle <PATH> \
  --stable-id <STABLE_ID> \
  --tier sig \
  --format sexpr \
  --max-tokens <N> \
  --budget <N> \
  [ --grammar-path <PATH> ]
```

## Positional Arguments

- `PATH`: Path to the source file to parse. Must be readable and within the allowed directory (cwd or repo root).

## Required Flags

- `--stable-id <STABLE_ID>`: The stable identifier to locate within the file. Scoped to `PATH`; does not search across files.
- `--tier sig`: The only supported tier in v1. Other values return a typed unsupported-tier result.
- `--format sexpr`: The only supported output format in v1. Other values return a typed unsupported-format result.
- `--max-tokens <N>`: Maximum tokens allowed in the result. This is the bridge/operator ceiling.
- `--budget <N>`: Token budget for included chunks. This gates which chunks are included in the bundle.

## Optional Flags

- `--grammar-path <PATH>`: Custom grammar directory for language discovery.
- `--quiet`: Suppresses non-error output. Not typically used in the bridge path but supported for CLI consistency.

## Budget Semantics

The effective inclusion limit is `min(budget, max_tokens)`.

- `--budget` controls which chunks are included in the bundle based on their `estimated_tokens`.
- `--max-tokens` is the bridge/operator result ceiling.
- A chunk with `estimated_tokens > budget` is omitted with reason `over_budget`.
- A chunk with `estimated_tokens > max_tokens` is also omitted, even if it fits within `budget`.
- The sum of `estimated_tokens` for included chunks never exceeds `min(budget, max_tokens)`.
- `estimated_tokens` reported for omitted chunks remains the true estimate, never capped.

Example:
```text
tree-sitter-context bundle src/lib.rs \
  --stable-id named:abc123... \
  --tier sig \
  --format sexpr \
  --max-tokens 5000 \
  --budget 500
```

This enforces a 500-token included-chunk budget while preserving true estimates for oversized omitted chunks, and guarantees the total result does not exceed 5000 tokens.

## Output

### Success (exit 0)

Canonical S-expression bytes written to stdout per `docs/plans/sexpr-canonical-form-v1.md`.

Result types:
- **Bundle**: `(bundle (version 1) (path ...) (cells ...) (provenance ...))`
- **not_found**: `(not_found (path ...) (stable_id ...) (reason ...) (provenance ...))`
- **ambiguous_stable_id**: `(ambiguous_stable_id (path ...) (stable_id ...) (candidates ...) (reason ...) (provenance ...))`
- **exhausted**: `(exhausted (path ...) (stable_id ...) (omitted ...) (provenance ...))`
- **unknown_cross_file**: `(unknown_cross_file (path ...) (stable_id ...) (reason "v1-non-goal"))`

### Process Failure (non-zero exit)

- Unreadable or non-existent `PATH`
- Path outside allowed directory
- Missing language grammar for file type
- Internal parser error

Error text written to stderr. Never encoded as stdout S-expression.

## Tier Values

- `sig`: Signature tier. Includes function signatures, type signatures, and struct/enum declarations. The only v1 tier.
- Reserved: `impl`, `body`, `full`, `doc`, `call`. Must return typed unsupported-tier result if requested in v1.

## Output Formats

- `sexpr`: Canonical S-expression. The only v1 format consumed by pi-mono.
- `json`: Reserved for debug/development only. Not part of the pi-mono bridge contract.

## Path Validation

- Paths are normalized before validation.
- Symlinks are resolved where practical.
- Path traversal outside the cwd/repo root is rejected before parsing.

## Stable ID Lookup

- Lookup is scoped to a single file (`PATH`).
- Zero matches → `not_found`.
- One match → bundle result (or `exhausted` if budget prevents inclusion).
- Multiple matches → `ambiguous_stable_id` with all candidates.
- v1 does not assume global stable ID uniqueness.

## Relationship to Existing Commands

This contract does not modify the existing `tree-sitter context` command:

```text
tree-sitter context <FILE> [--old <OLD>] [--symbols] [--budget N] [--quiet] [--grammar-path PATH]
```

The existing JSON-output context command remains available and unchanged. The `tree-sitter-context bundle` command is a dedicated binary for the pi-mono R0 bridge.

## Versioning

The v1 contract is frozen. Future changes must:
1. Introduce a new `--format` value or `--version` flag.
2. Update `docs/plans/sexpr-canonical-form-v1.md` with a new version section.
3. Maintain backward compatibility or use explicit opt-in flags.

## Error Codes

Process failures map to these stable stderr prefixes:

| Error | Prefix | Example |
|-------|--------|---------|
| unreadable_path | `error: unreadable path` | `error: unreadable path: /etc/shadow` |
| path_traversal | `error: path outside allowed root` | `error: path outside allowed root: ../../foo` |
| missing_language | `error: no language grammar` | `error: no language grammar for .xyz` |
| invalid_stable_id | `error: invalid stable_id` | `error: invalid stable_id format: bad:id` |
| unsupported_tier | `error: unsupported tier` | `error: unsupported tier: body` |
| unsupported_format | `error: unsupported format` | `error: unsupported format: xml` |

## Reserved for Future Versions

- Cross-file resolution via `--project` or `--index` flags.
- Graph snapshot integration (`graph_snapshot_id` other than `"unknown"`).
- Orientation freshness tracking (`orientation_freshness` other than `"unknown"`).
- Daemon mode or persistent connection.
- MCP server integration.
