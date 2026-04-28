## Remote safety

Never push to `upstream`. Only push branches to `origin`, which must be the
user's fork. If remote configuration is ambiguous, stop and ask before pushing.

## Nested repositories

This repository contains a nested git repository at `pi-mono/` (a full `.git` directory inside the subdirectory, not a git submodule). This is intentional: `pi-mono` is a separate project that is integrated into this tree-sitter branch as a nested checkout.

### Rules for nested repo work

- **Treat `pi-mono/` as a separate git repository.** Run all git commands (status, add, commit, push) from inside `pi-mono/` when working on its files.
- **Never run `git add -A` or `git add .` from the tree-sitter root** — this would stage pi-mono's `.git` directory or its working files into the parent repo.
- **Commit tree-sitter and pi-mono changes separately.** The parent repo tracks pi-mono as a plain directory; changes inside it appear as modified content in `git status` but must be committed from within `pi-mono/`.
- **When pushing branches:** Push the tree-sitter branch from the tree-sitter root, and push the pi-mono branch from `pi-mono/`.
- **Pi-mono uses `npm run check` for its pre-commit hook.** If the hook fails on unrelated packages (e.g., `web-ui`), use `--no-verify` only after confirming the failure is pre-existing and unrelated to your changes.

## Project docs

- [README.md](README.md) - project overview and public links.
- [CONTRIBUTING.md](CONTRIBUTING.md) - contributor guide entry point.

### Active plans (tree-sitter-context)

- [docs/plans/tree-sitter-context-rfc-2026-04-24.md](docs/plans/tree-sitter-context-rfc-2026-04-24.md) - experimental `tree-sitter-context` RFC and high-level design record.
- [docs/plans/tree-sitter-context-hardening-implementation-plan-2026-04-25.md](docs/plans/tree-sitter-context-hardening-implementation-plan-2026-04-25.md) - hardening the context prototype (partially completed).

### Recently completed

- [docs/plans/2026-04-28-002-feat-ast-aware-read-tool-plan.md](docs/plans/2026-04-28-002-feat-ast-aware-read-tool-plan.md) - AST-aware read tools (`read_ast_outline`, `read_symbol`, `read_ast_delta`) with session memory.
- [docs/plans/2026-04-28-001-feat-semantic-session-compaction-plan.md](docs/plans/2026-04-28-001-feat-semantic-session-compaction-plan.md) - semantic compaction for session state management.
- [docs/plans/2026-04-27-002-feat-incremental-invalidation-plan.md](docs/plans/2026-04-27-002-feat-incremental-invalidation-plan.md) - incremental invalidation with change classification.
- [docs/plans/2026-04-27-001-feat-r3-god-nodes-postprocess-plan.md](docs/plans/2026-04-27-001-feat-r3-god-nodes-postprocess-plan.md) - god-node postprocessing.
- [docs/plans/2026-04-26-003-feat-r2-orientation-handshake-plan.md](docs/plans/2026-04-26-003-feat-r2-orientation-handshake-plan.md) - orientation handshake protocol.
- [docs/plans/2026-04-26-002-feat-r1-repo-map-plan.md](docs/plans/2026-04-26-002-feat-r1-repo-map-plan.md) - repository map generation.
- [docs/plans/2026-04-26-001-feat-r0-context-firewall-plan.md](docs/plans/2026-04-26-001-feat-r0-context-firewall-plan.md) - context firewall.

### Deferred / follow-up

- [docs/plans/tree-sitter-context-follow-up-plan-2026-04-25.md](docs/plans/tree-sitter-context-follow-up-plan-2026-04-25.md) - deferred follow-up work from the context RFC and branch review.

### Knowledge base

- [docs/solutions/](docs/solutions/) - compound-engineering knowledge store for documented solutions and review checkpoints, organized by category with YAML frontmatter. Check relevant entries before related work.
