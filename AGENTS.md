## Remote safety

Never push to `upstream`. Only push branches to `origin`, which must be the
user's fork. If remote configuration is ambiguous, stop and ask before pushing.

## Repository layout

`pi-mono` is a separate project and should live as a sibling checkout, not as a
nested repository inside this tree-sitter fork:

```text
~/Work/github/
├── tree-sitter/
└── pi-mono/
```

### Rules for sibling repo work

- **Keep `tree-sitter/` and `pi-mono/` as separate git repositories.** Run git
  commands from the repo that owns the files you are changing.
- **Never place `pi-mono/` back under the tree-sitter root** unless the user
  explicitly asks to convert it into a formal submodule or subtree.
- **Commit tree-sitter and pi-mono changes separately.** The tree-sitter fork
  owns the Rust `tree-sitter-context` implementation; pi-mono owns the coding
  agent integration.
- **When pushing branches:** Push the tree-sitter branch from `tree-sitter/`,
  and push the pi-mono branch from `../pi-mono/`.
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
