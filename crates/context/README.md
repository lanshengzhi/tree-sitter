# tree-sitter-context

Experimental library for extracting agent-oriented code context from tree-sitter syntax trees.

This crate is currently a prototype inside the tree-sitter workspace. It focuses on:

- syntax-aware chunks with stable identities,
- old/new snapshot invalidation,
- optional tags-based symbol extraction,
- budgeted chunk bundles,
- machine-readable JSON output shapes.

The public contract is still experimental. See the repository planning documents under `docs/plans/` before relying on the schema outside local development.
