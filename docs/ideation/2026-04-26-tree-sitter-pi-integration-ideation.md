---
date: 2026-04-26
topic: tree-sitter-pi-integration
focus: "Integrating tree-sitter capabilities into the pi coding agent"
mode: repo-grounded
---

# Ideation: Integrating tree-sitter capabilities into pi coding agent

## Grounding Context

**Codebase Context:**
- tree-sitter: Rust/C parser generator + incremental parsing library. Core in `lib/` (C + Rust/Web bindings), Rust workspace in `crates/` (cli, context, generate, highlight, tags, loader). Experimental `tree-sitter-context` crate under active hardening (see `docs/plans/`).
- pi-mono: TypeScript monorepo inside `pi-mono/`. Packages: `coding-agent`, `agent`, `ai`, `mom`, `pods`, `tui`, `web-ui`. CLI agent with read/bash/edit/write tools and session management.
- **Language boundary**: pi-mono is TypeScript/Node; tree-sitter core is Rust/C. No existing bridge or binding consumption in pi-mono.
- **Maturity gap**: `tree-sitter-context` is experimental and flagged for hardening — not yet a stable API to depend on.
- **No integration seam**: pi-mono's `coding-agent` currently lacks AST-aware code intelligence; it operates on raw text/files.

**Past Learnings:**
- Position as a low-level primitive, not a product (RFC explicitly rejects MCP server, SQLite graph, vector search in v1).
- Treat invalidation as a first-class agent concern (must tell agent which chunks changed, why, confidence).
- Stable identity is a hard contract requirement (DefaultHasher silently collapses duplicate names).
- Budget honesty is non-negotiable (capping estimated_tokens at max_tokens violates budget contract).
- CLI contract gaps block automation (flags exposed without honored behavior must be implemented or hidden).

**External Context:**
- Prior art: Aider repo-map (Python, tree-sitter + PageRank), CodeSift (TypeScript MCP server, 61-95% token reduction), CODESTRUCT (ACL 2026, -12-38% tokens), cAST (CMU ACL 2025, +4.3 Recall@5).
- Market signals: MCP is default integration layer in 2026. Token economics dominate design.
- Cross-domain analogies: compiler front-end/back-end separation (LLM reasons, tree-sitter formats), PageRank for code authority.

## Ranked Ideas

### 1. AST-Aware Read Tool with Incremental Invalidation
**Description:** A new `read-ast` tool that returns semantic chunks with stable IDs instead of raw file text. Integrate with tree-sitter-context's incremental invalidation to tell the agent which chunks are stale after edits, so only changed chunks need refreshing.
**Warrant:** `direct:` tree-sitter-context RFC defines these primitives (changed_ranges → affected chunks, stable IDs, budgeted bundles); pi's current read tool returns raw text causing token waste per RFC Background section.
**Rationale:** This directly addresses the RFC's stated goal of "让上层工具少读无关代码" by replacing whole-file reads with precise semantic units. The invalidation primitive solves the re-read problem.
**Downsides:** Requires stable ID contract to be hardened first; Node/Rust bridge needed; agents must understand chunk-based context.
**Confidence:** 90%
**Complexity:** High
**Status:** Unexplored

### 2. Semantic Session Compaction
**Description:** Replace current session compaction with tree-sitter-based semantic summarization. Keep full signatures of changed functions, compress unchanged bodies to summaries, and track added/removed/modified symbols.
**Warrant:** `direct:` pi has session compaction (README "Sessions > Compaction"); tree-sitter-context does AST boundary chunking and symbol extraction. Session bloat is a known token cost driver.
**Rationale:** Compaction is already a core pi feature. Adding AST-awareness makes it structure-preserving instead of text-truncating, keeping more useful context within the same token budget.
**Downsides:** Compaction logic is already complex; adding AST traversal may increase latency.
**Confidence:** 85%
**Complexity:** Medium
**Status:** Unexplored

### 3. Skill System with AST Query Resolution
**Description:** Extend pi's skill system so skills can declare tree-sitter query patterns (e.g., "I work on function definitions"). Pi resolves these to concrete code spans, avoiding LLM reads of irrelevant code.
**Warrant:** `direct:` pi has a skill system (README "Skills"); tree-sitter has a query language (`Query`/`QueryCursor` in Rust bindings, also in web bindings). Current skills are text-prompt based.
**Rationale:** Skills are a natural integration point — they're already modular and user-authored. Adding AST query resolution makes them semantically precise without changing the skill authoring experience.
**Downsides:** Requires tree-sitter grammar availability for each language; query authoring has a learning curve.
**Confidence:** 80%
**Complexity:** Medium
**Status:** Unexplored

### 4. First-Class Repo Map via Symbol Graph
**Description:** Maintain an in-memory symbol graph extracted via tree-sitter tags queries. Provide commands like `/find-callers`, `/find-defs`, `/impact-analysis` that use the graph instead of grep.
**Warrant:** `external:` Aider repo-map proves this pattern works (PageRank on import/reference graph); `direct:` tree-sitter has tags extraction (`crates/tags/src/tags.rs`) and loader (`crates/loader/src/loader.rs`).
**Rationale:** Navigation in large repos is a major pain point. A living symbol graph avoids repeated grep and gives the agent structural awareness of dependencies and impact.
**Downsides:** Requires indexing on startup; monorepos may have blind spots (per Aider's known pain points); stale graph after edits unless incremental.
**Confidence:** 75%
**Complexity:** High
**Status:** Unexplored

### 5. Edit Validation with AST-Aware Boundaries
**Description:** Validate that `old_string`/`new_string` edit boundaries align with AST node boundaries before applying. Optionally add a lightweight post-processor for whitespace-perfect merging.
**Warrant:** `external:` Claude Code uses exact-string-replace for near-zero corruption risk; Cursor's apply model uses a small model for formatting; `direct:` pi has edit/write tools that operate on raw text.
**Rationale:** Edit corruption is a real risk in agent workflows. AST boundary validation adds structural guardrails without changing the edit tool interface, making pi more reliable.
**Downsides:** Strict AST alignment may reject valid edits that cross node boundaries; adds latency to every edit.
**Confidence:** 70%
**Complexity:** Medium
**Status:** Unexplored

### 6. Token-Budgeted Interactive Context Display
**Description:** In interactive mode, show a sidebar of semantic chunks with estimated token counts. Users can pin/unpin chunks; pi automatically includes the most relevant ones within the model's context window.
**Warrant:** `direct:` tree-sitter-context has budgeted bundle logic (`--budget <TOKENS>`); pi has interactive mode with editor/commands/sidebar. Token economics dominate LLM agent design (per external research).
**Rationale:** Makes the invisible token budget visible and controllable. Users can see exactly what context is eating tokens and optimize it.
**Downsides:** Requires all other semantic chunking to be in place first; UI complexity; TUI implementation effort.
**Confidence:** 65%
**Complexity:** Medium
**Status:** Unexplored

## Rejection Summary

| # | Idea | Reason Rejected |
|---|---|---|
| 7 | Tree-Sitter Query Language for Extensions | Extension system is UI-focused; AST query hooks are a niche feature compared to core read/edit/session workflows. Better as Phase 2 enhancement after core tooling is AST-aware. |
| 8 | MCP Bridge for pi → External Agents | Subject-replacement risk: shifts pi from CLI coding agent to MCP server provider. RFC explicitly excludes MCP from v1 scope. Better as a product strategy brainstorm, not a tree-sitter integration. |
