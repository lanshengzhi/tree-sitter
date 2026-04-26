---
title: "tree-sitter-context Graph R1 Contract"
type: contract
status: active
date: 2026-04-26
origin: docs/plans/2026-04-26-002-feat-r1-repo-map-plan.md
---

# tree-sitter-context Graph R1 Contract

## Commands

All graph commands are additive subcommands of `tree-sitter-context`:

```text
tree-sitter-context graph build   [--repo-root <PATH>] [--grammar-path <PATH>] [--quiet]
tree-sitter-context graph update  [--repo-root <PATH>] [--grammar-path <PATH>] [--quiet]
tree-sitter-context graph status  [--repo-root <PATH>]
tree-sitter-context graph verify  [--repo-root <PATH>]
tree-sitter-context graph diff    [--repo-root <PATH>] --from <SNAPSHOT_ID> --to <SNAPSHOT_ID>
tree-sitter-context graph clean   [--repo-root <PATH>]
```

## Namespace Isolation

- Graph commands do not change `bundle` argument parsing, S-expression output, result variants, or R0 pi-mono bridge behavior.
- Graph success output is stable JSON on stdout.
- Process-level failures use non-zero exit and stderr, following the R0 CLI contract.

## Snapshot Identity

- `graph_snapshot_id` is a deterministic XXH3-128 hex digest of canonical JSON bytes.
- Canonical form sorts files, nodes, edges, and diagnostics by deterministic keys.
- Timestamps, absolute paths, and operational metadata are excluded from the hash.
- Changing `schema_version` changes the snapshot ID.

## Store Semantics

- Snapshots are stored as canonical JSON under `.tree-sitter-context-mcp/<SNAPSHOT_ID>.json`.
- HEAD is a small text file at `.tree-sitter-context-mcp/HEAD` containing the active snapshot ID.
- Writes use temp-file + atomic rename.
- HEAD is updated only after the target snapshot verifies as readable.
- `clean` removes unreachable snapshots but never deletes the current HEAD target.

## Diff Buckets

- `changed_files`: added, removed, or modified by content hash.
- `changed_nodes`: added, removed, or modified with signature/content hash evidence.
- `changed_symbols`: added or removed by name and definition status.
- `changed_edges`: added, removed, or modified by edge status.
- `postprocess_unavailable`: true when severe orientation changes (e.g., rename/move) cannot be classified without god-node/community data.

## Cross-File Edges

- Edges carry explicit status: `confirmed`, `ambiguous`, `unresolved`, `unsupported`.
- `confirmed`: exactly one unambiguous definition candidate.
- `ambiguous`: multiple candidates exist; candidates are preserved in deterministic order.
- `unresolved`: no candidates found.
- `unsupported`: language or config lacks reference capability.

## Typed Errors

- `MissingSnapshot`: snapshot ID not found in store.
- `CorruptedSnapshot`: unreadable or unparseable snapshot file.
- `SchemaMismatch`: expected vs. actual schema version mismatch.
- `WriteFailure`: temp file, atomic rename, or fsync failure.
- `PostprocessUnavailable`: god-node/community data absent for severe orientation classification.

## Compatibility

- R0 `bundle` contract remains intact.
- No daemon, MCP server, stdio JSON-RPC, N-API, or WASM bridge introduced.
- No R3 tools (`/find-callers`, `/find-defs`, etc.) exposed.
