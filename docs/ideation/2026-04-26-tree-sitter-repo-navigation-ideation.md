---
date: 2026-04-26
topic: tree-sitter-repo-navigation
focus: "Agent-friendly repo orientation via tree-sitter-context-mcp — pre-built graph, anti-noise primitives, S-expression IO, replace pi-mono Auto-compact"
mode: repo-grounded
---

# Ideation: tree-sitter-context-mcp — Agent-friendly repo navigation infrastructure

This doc complements [`2026-04-26-tree-sitter-pi-integration-ideation.md`](./2026-04-26-tree-sitter-pi-integration-ideation.md), which surveyed broad integration angles (read-tool, compaction, skills, repo map, edit, TUI). This doc narrows to one ambitious bet: **build an MCP-shaped service that gives an AI coding agent a clear pre-built map of the repo and lets it navigate by structure, not text**, even if that means rewriting parts of pi-mono. The first ideation answered "what could we do with tree-sitter inside pi"; this one answers "if we had to design the agent's primary code-perception layer from scratch, what would it be?"

## Grounding Context

### Codebase Context
- `crates/context/` exposes chunking, stable IDs, ranges, invalidation, budgeted bundles — single-file scope; cross-file is a v1 non-goal per the RFC.
- `crates/tags/` extracts symbols (defs/refs/calls) per language; no cross-file resolution.
- `lib/binding_web/` exposes `TreeCursor` / `Node` for in-tree walking; nothing repo-scoped.
- `pi-mono/packages/coding-agent/` ships `read / write / edit / bash / grep / find / ls` and **no navigation tools**. Sessions are per-cwd JSONL trees with no repo-scoped state, no concept of a "current focus", and no memory of explored regions.
- The integration seam between Rust and TypeScript does not exist yet.

### Past Learnings (binding constraints — see [`docs/solutions/workflow-issues/tree-sitter-context-branch-review-2026-04-25.md`](../solutions/workflow-issues/tree-sitter-context-branch-review-2026-04-25.md))
- `StableId` uses `DefaultHasher` and silently collapses repeated names — must be disambiguated before any cross-file relation key relies on it.
- `estimated_tokens` is capped at `max_tokens`, so oversized chunks look budget-safe — budget honesty must be re-established.
- Snapshot invalidation uses `Tree::changed_ranges` on independently parsed trees and may silently degrade — agent-side code must surface "synthesized / degraded" rather than treat it as precise.
- CLI flags `--budget`, `--quiet`, `--grammar-path` are accepted but not honored; invalidation buckets lack per-chunk reason / strategy / confidence; output ordering is non-deterministic.
- **Design rule**: every new primitive must surface `reason / strategy / confidence / omissions` like invalidation does.

### External Context
- **LSP** (`callHierarchy`, `workspace/symbol`, `references`, `implementation`, `typeHierarchy`) is the de-facto code-navigation primitive set; **SCIP** (Sourcegraph) is its portable static-index counterpart with `is_implementation` / `overrides` flags. **Stack Graphs** (GitHub, tree-sitter-based) is file-incremental but def/ref-only and language-limited.
- **Aider repo-map** uses tree-sitter + bipartite file graph + PageRank with chat-file 100x weighting; documented monorepo hub-node dominance failure at GH#2405 (closed stale).
- **Cline three-layer hybrid** (ripgrep + dir traversal + AST defs) outperforms any single method by ~8%.
- **2026 UX convergence**: light map for orientation → targeted nav per point; trying both in one pass overfills context.

### Reference Projects (validated via context7)
- **`code-review-graph`** (tirth8205, /tirth8205/code-review-graph): tree-sitter + SQLite knowledge graph, `build` / `update` (sub-2s incremental via git diff + dependent-import propagation + SHA-256 hash), 19 languages + Jupyter, MCP integration, **8.2x token reduction** via blast-radius analysis. Real-world: 1122 files → 6285 nodes / 27117 edges. Postprocess (flows / communities) is separable: `--skip-flows`, `--skip-postprocess`.
- **`graphify`** (safishamsi, /safishamsi/graphify): pre-built `graph.html` + `GRAPH_REPORT.md` + `graph.json` + SHA256 cache. MCP tools: `query_graph(question, mode="bfs|dfs", depth=3, token_budget=2000)`, `get_neighbors(label, relation_filter)`, `get_community(community_id)`, **`god_nodes(top_n=10)` — names hub nodes explicitly so they don't silently dominate the map** (the direct fix for Aider's hub-dominance bug), `shortest_path(source, target, max_hops=8)`.

### Reference Document (~/Downloads/Tree-sitter 改造 MCP_Skill 方案.md)
- **Claude Code five-layer compaction pipeline** (revealed by Anthropic's 2026-03-31 NPM source leak): Budget Reduction → Snip → Microcompact → Context Collapse → **Auto-compact**. The first four are deterministic; **Auto-compact is LLM-driven and demonstrably poisonable via prompt injection** (Adversa AI audit). Replacing Layer 5 with deterministic graph-based compaction is the single highest-leverage security + economic move.
- **S-expression as serialization format**: 22% shorter than JSON (Claude Sonnet 4.6), 5–64x more compact than XML on production codebases (arXiv:2604.13108). LLMs trained on code recognize the syntactic hierarchy natively — CoT path length shrinks, attention sharpens.
- **Semantic Diff (AST-level)**: 68–86% token-noise reduction vs git's line diff. Output: `(modified (class "AuthService") (added_method "verifyMFA"))`. On a 200K context this frees ~50K for reasoning.
- **CodeStruct** (Amazon Science, arXiv:2604.05407): structured action spaces over named entities cut SWE-Bench tokens 12–38%, but more strikingly raised GPT-5-nano repair rate by **+20.8%** — low-tier models benefit more, which democratizes AI coding.
- **Codebase-Memory** (arXiv:2603.27277): single C/C++ binary + SQLite + 66 languages, 14 strongly-typed structural query tools across Discovery / Search / Retrieval / Tracing categories, sub-millisecond graph queries.
- **Two Corrections Rule**: when the same problem requires more than 2 corrections in one session, the context is poisoned with failed attempts and must be cleared or delegated. Currently a developer heuristic; tractable as an automated trigger.

### Subject Sharpening (from refinement)
- **Anti-noise is the primary design target**, not coverage. "Map" success = "agent's uncertainty about where to look drops"; not "how much we returned." Aider's hub-dominance bug is the canonical *negative example*, not a small flaw.
- **The map is pre-generated as a build artifact** (graph.json / SQLite), updated incrementally on code change, never built at query time.
- **The consumer is the LLM, not the human and not pi-mono**. Pi-mono is the bridge. "Model-Provider-friendly" is a load-bearing constraint: prompt-cache stability, structured-not-text outputs, layered output tiers, verifiable handles, explicit negative signals, replace-not-augment of the existing tool surface, graph-aware compaction.
- **Pi-mono's existing context-management mechanisms are in scope to replace**, including the Claude-Code-style five-layer compaction pipeline if applicable.

## Ranked Ideas

### R0. Agent Interface Contract / Context Firewall (pi-mono replacement layer)
**Description:** A re-shaped pi-mono integration layer that treats the LLM as the customer and tree-sitter-context-mcp as the only legitimate source of code structure. Concretely: (a) the system prompt embeds a cache-stable `<2k token` orientation block (architecture summary + god_nodes + community list + entry points), positioned to hit Anthropic's prompt cache for whole sessions; (b) `grep / find / ls` are removed from the tool surface and replaced with the R3 graph-native primitives; (c) **all tool results are serialized as S-expressions** (22% smaller than JSON, 5–64x smaller than XML; matches the LLM's pretrained code distribution); (d) every result carries a stable `node://lang::Module::Type::method@v123` handle the agent uses for citations; (e) every primitive returns explicit negative signals (`(not_found ...)`, `(exhausted depth 5)`, `(ambiguous (candidates ...))`) instead of empty results; (f) **the Claude-Code-style Auto-compact (Layer 5) is replaced with deterministic graph-aware compaction** — old messages keep their handles, bodies collapse to signatures, no LLM-written history summary; (g) Two-Corrections Rule is automated — repeated correction on the same node triggers `should_reorient(reason: poison|drift|loop)`; (h) format coercion is the security perimeter — LLM input arriving from untrusted code (comments, docstrings, dependency files) is structurally bounded inside `@doc` nodes that cannot escape into the instruction stream.
**Warrant:** `external:` Anthropic's 2026-03-31 source leak (analyzed in arXiv:2604.14228 and Adversa AI audit) shows Layer 5 Auto-compact is LLM-driven and prompt-injection-vulnerable; layers 1–4 are deterministic and safe. `external:` S-expression compactness 22% < JSON, 5–64x < XML on real codebases (arXiv:2604.13108). `external:` CodeStruct (arXiv:2604.05407): structured action spaces lift GPT-5-nano repair rate by +20.8% — low-tier-model leverage is the democratization argument. `direct:` pi-mono coding-agent's tool surface is `read / write / edit / bash / grep / find / ls` with no navigation primitives and per-cwd JSONL with no repo state — the gap that R0 fills.
**Rationale:** Without R0, R1–R5 ship as a "cool library agents can't actually use." R0 is the last mile from "graph done correctly" to "agent measurably less confused." Three benefits stack: token economy (cache hit + smaller serialization), reasoning quality (LLM reads what it was trained on), and security (Auto-compact poisoning becomes structurally impossible).
**Downsides:** Largest behavior change to pi-mono; demands replacing the existing tool surface and compaction logic, not augmenting them. S-expression serialization deviates from MCP idiomatic JSON (justifiable but visible). Cache-stable orientation block must be regenerated on graph rebuild — invalidation discipline is mandatory. Removal of `grep / find / ls` loses non-symbol text search (TODO comments, error strings); R3 must absorb these via `.scm` query packs (`todos.scm`, `error_sites.scm`).
**Confidence:** 90%
**Complexity:** High
**Status:** Unexplored

### R1. Three-stage pre-built graph: `tree-sitter-context-mcp graph build / update / postprocess`
**Description:** Mirrors `code-review-graph`'s proven shape. **`build`** parses every supported file once into nodes (AstCells), edges (calls, imports, refs, type relations), and stable_ids; persists to `.tree-sitter-context-mcp/graph.{db,json}` (SQLite WAL for queries; JSON snapshot for portability). **`update --base <ref>`** uses git diff to find changed files, applies XXH3 content-hash filtering, identifies dependent files via the existing import edges, and re-parses only the affected subgraph (target: <2s on real-world repos per `code-review-graph` benchmarks). **`postprocess`** is separable and skippable (`--skip-flows / --skip-communities`): community detection (Louvain), centrality (PageRank / fan-in-out), strata classification (R4), flow-slice extraction (R4). The CLI mirrors `cargo` ergonomics: `tree-sitter-context-mcp graph status / verify / clean`. Triggered automatically by an opt-in git hook (`post-commit` runs `update`; CI runs `build` cold).
**Warrant:** `external:` `code-review-graph` ships exactly this shape (build / update / postprocess split with `--skip-flows / --skip-postprocess`), 1122 files → 6285 nodes / 27117 edges, sub-2s incremental updates, 8.2x token reduction (context7 doc). `external:` Codebase-Memory (arXiv:2603.27277) uses SQLite for sub-millisecond graph queries on 66 languages. `direct:` `crates/context/src/invalidation.rs` already returns `unchanged|affected|added|removed` per chunk — the building blocks for incremental update exist; this idea generalizes from per-file to per-graph.
**Rationale:** The user's pre-built constraint moves cost from query-time (uncertain, tail-latent) to build-time (predictable, parallelizable, cache-friendly). Three-stage separation matters: `build` must be fast enough for first-run UX; `update` must be fast enough for git-hook UX; `postprocess` can be expensive because it's optional and pre-computed. Without separation, a single slow stage poisons all UX paths.
**Downsides:** Persistent on-disk graph adds storage cost (~10MB per 1k files at code-review-graph density) and a versioning concern (graph schema migrations). Git-hook auto-update is friction in monorepos with many concurrent branches; needs a per-worktree lockfile. Cold `build` on multi-million-file monorepos may exceed the "fast" target — needs a budgeted-streaming variant.
**Confidence:** 92%
**Complexity:** High
**Status:** Unexplored

### R2. `AstCell` + canonical symbol-path + typed `Provenance<T>` envelope
**Description:** One unit `AstCell { stable_id, kind, range, content_hash, language, symbol_path }` flows through every Rust↔TS boundary, every tool result, every session entry. The canonical symbol-path syntax is `lang::module::Type::method@arity` (or language-idiomatic equivalent), produced by tree-sitter tags + per-language conventions, durable across edits as long as the symbol's path doesn't change. Every primitive returns `Provenance<T> { value, reason, strategy ∈ {TagsQuery|ChunkedWalk|XrefIndex|GrepFallback|StaleCache|...}, confidence: 0..1, omissions: [{stable_id, reason}] }` — Rust type system enforces honesty (a primitive returning a raw value won't compile). The TS bridge mirrors as a TypeScript discriminated union for native ergonomics on the agent side.
**Warrant:** `direct:` past-learnings doc explicitly codifies "every new primitive must surface reason / strategy / confidence / omissions"; `BundleOutput.OmissionReason` is the existing precedent. `direct:` three near-isomorphic shapes already exist (context StableId, tags Symbol, binding_web Node) waiting to be unified. `external:` SCIP and LSP both standardize on symbol monikers because aider's failure mode is partly stale string refs.
**Rationale:** R0–R5 all read this envelope. Adding a primitive that lies becomes a compile error. AstCell is the citation handle the agent writes into PR descriptions and docs/solutions/ entries — durably re-findable across edits and refactors. Without R2 the rest of the system is ten dialects pretending to share a language.
**Downsides:** Demands fixing `StableId` duplicate-name collapse first (currently P1) — until then AstCell is unsafe as a cross-file relation key. Symbol-path schema is a long-lived design choice — getting it wrong is expensive. Typed Provenance adds boilerplate; calibration data for `confidence` requires evaluation harness work that doesn't exist yet.
**Confidence:** 88%
**Complexity:** High
**Status:** Unexplored

### R3. Query primitive set, four categories, S-expression output, every call budgeted
**Description:** A coherent set of MCP tools shaped by Codebase-Memory's four-category taxonomy plus extensions from graphify and the reference document. **Discovery**: `get_ranked_architecture(focus_module?, token_limit)` returns the Louvain-clustered backbone view; `god_nodes(top_n)` names hub nodes explicitly (the Aider hub-dominance fix); `community(id)` lists members. **Search**: `query_semantic_symbols(intent, types, token_limit)` resolves natural-language intent → ranked symbol IDs (not source). **Retrieval**: `get_context_bundle(target_symbol_id, trace_depth, max_tokens, tier ∈ {id|sig|sig+doc|full})` returns full body of target plus stripped-to-signature neighbors at the requested depth. **Tracing**: `get_semantic_diff_impact(target_symbol_id, compute_transitive)` runs reverse SQL BFS for blast radius; `shortest_path(source, target, max_hops)` for relationship discovery. **Verification**: `assert_callgraph(caller_id, callee_name)` (returns confirmed | not_found | ambiguous + spans + confidence); `missing_symbols(query)` (typo / renamed / test-only / deleted-at-sha). **Edit**: `safe_edit(target_symbol_id, replacement_s_expr | source)` validates the proposed change with tree-sitter pre-write — broken brackets / unclosed scopes are rejected with compiler-grade errors for LLM retry. **Meta**: `should_reorient(session_history)` automates the Two-Corrections Rule. Every primitive: (a) takes an explicit `token_budget`, (b) returns S-expressions, (c) emits negative signals not empty results, (d) is wrapped in `Provenance<T>`.
**Warrant:** `external:` Codebase-Memory four-category surface (Discovery/Search/Retrieval/Tracing) is the validated taxonomy on 14 strongly-typed tools (arXiv:2603.27277). `external:` graphify's `god_nodes / community / shortest_path` directly fix Aider's hub-dominance and add path discovery. `external:` jCodeMunch-MCP `get_ranked_context(token_budget)` achieves 95–99% token reduction on Express.js / FastAPI via centrality + greedy packing. `external:` tree-sitter pre-write syntax validation in SemanticPrune-MCP's `apply_safe_syntax_edit`. `direct:` `crates/tags` already enumerates symbols per file; `crates/context` already does budgeted bundles — the building blocks exist.
**Rationale:** This is the agent-facing surface: ~10 primitives with sharp semantics replace the current "grep + read + hope." Each primitive is opinionated about its return shape; the agent stops planning multi-call walks for common questions because the right primitive exists. S-expression output cuts tokens; budget enforcement cuts cost; negative signals cut hallucinations. Verification primitives (`assert_callgraph`, `missing_symbols`, `safe_edit`) convert three of the highest-frequency agent failure modes into typed errors.
**Downsides:** Surface is broader than minimum-viable — risks scope sprawl. Some primitives need cross-file resolution (`shortest_path`, `assert_callgraph` for cross-file calls) which is a `crates/context` v1 non-goal — must either escalate that or honestly emit `unknown_cross_file` confidence-low. Natural-language intent in `query_semantic_symbols` requires either embedding lookup or LLM call — adds a dependency. `safe_edit` is a non-trivial replacement of pi-mono's `edit` tool with an opinionated counterpart.
**Confidence:** 86%
**Complexity:** High
**Status:** Unexplored

### R4. Reframe horizontal/vertical: strata for vertical, flow-slice for horizontal
**Description:** Reject AST-sibling as horizontal and caller→callee as vertical. **Vertical = abstraction strata** (route-handler → service → repository → driver / adapter), inferred from tree-sitter tag patterns plus a per-project ruleset (`queries/<lang>/strata.scm`); R3 exposes `up(focus, levels) / down(focus, levels)` that walk strata, automatically skipping cross-cutting glue (logging, metrics, framework). **Horizontal = flow-slice membership** — the set of nodes touched by a single request lifecycle, job, PR, or test trace; modeled as a sparse stage-blocking matrix (rows = scenes / steps; columns = actors / functions / modules; cells mark entry / exit). The agent navigates with `flow_slice(name, mode ∈ {static|trace})`. Static construction uses graph reachability + handler heuristics; trace-backed construction reads runtime data when available (test harness instrumentation, OpenTelemetry, request logs). Surface in R3 as Discovery primitives (`list_flows`, `get_flow_slice`).
**Warrant:** `reasoned:` Call graphs collapse layers — a controller `calls` a logger and a database equally, but developers don't think that way; layered nav skips glue automatically. `external:` Cline's three-layer hybrid winning ~8% over single methods hints layered abstraction beats flat call graphs for human-style nav. `external:` Stage-management blocking sheets (theatre, since 19th c.) are the validated structural answer to "many actors, sequenced scenes, who's-with-whom matters more than spatial position." `reasoned:` "业务流程的某个片段" appears literally in the user's brief — the call-graph primitive set cannot answer "what runs in the same lifecycle as this point" without a flow-slice abstraction.
**Rationale:** This is the literal answer to the user's "横/纵向关系探索" — neither dimension is well-served by AST primitives alone. Strata answer "what's the next abstraction level?" Flow slices answer "what runs in the same lifecycle as this point?" Both are questions the LSP primitive set cannot answer; both are first-class to the user's brief. The reframe is the value: not a new tool, a new vocabulary the user already speaks.
**Downsides:** Strata require per-project rules (some friction); wrong rules give noisy results — needs sane defaults per language family (web framework, CLI, embedded, etc.). Flow slices need traces (runtime data) or static flow analysis to populate the matrix; static-only construction has gaps for dynamic dispatch and DI containers. Both add implementation surface beyond a minimum-viable graph.
**Confidence:** 78%
**Complexity:** Medium
**Status:** Unexplored

### R5. Exploration overlay + blast-radius graded invalidation
**Description:** A per-repo append-only overlay layered over R1's graph: every read / nav / edit appends `(timestamp, AstCell, action, outcome, tokens)` to `.tree-sitter-context-mcp/exploration.jsonl` (gitignored, shared across sessions on the same repo). Three uses: (a) **deduped reads** — when about to re-read a node, return "you saw this at turn 12, cost 180 tok, still unchanged via invalidation"; (b) **second-visit memo** — second time a node is bundled, replace body with structural memo (signature + exports + neighbors); (c) **session-N orientation prior** — N-session warm cache informs the cache-stable orientation block in R0. **Blast-radius graded invalidation** runs on edit: an interval tree per file maps changed byte ranges to AstCells, then emits **inner zone** `definitely-affected (re-parse-subtree, confidence: high)`, **middle zone** `likely-affected (signature-changed, confidence: medium)`, **outer zone** `speculative (git co-changed historically, confidence: low)`. The Provenance envelope of every nav primitive carries the invalidation zone of its source data. Optional pre-invalidation: when the agent declares an intended edit before applying, predict blast-radius in parallel — high-confidence prediction warms downstream nav.
**Warrant:** `direct:` `crates/context/src/invalidation.rs` already returns `unchanged|affected|added|removed` per chunk with reason+confidence — this idea exposes that signal in graded form to the overlay. `external:` `code-review-graph` literally calls its mechanism "blast-radius analysis" and ships sub-2s incremental update via SHA-256 + dependent-import propagation (context7 doc). `external:` FEMA / NOAA blast-radius maps combine deterministic-physics inner zone with stochastic-prior outer zone with explicit confidence rings — direct structural mapping to type-flow (deterministic) + git co-change (probabilistic). `direct:` past-learnings flag `Tree::changed_ranges on independently parsed trees may silently degrade` and `invalidation lacks per-chunk reason/strategy/confidence` — this is the literal application of the design rule.
**Rationale:** Closes the silent-degradation hole, makes invalidation observable to the TUI, and prevents the classic post-edit failure mode where the agent reasons over a pre-edit signature still in its prompt. Compounds across sessions — agent gets durably smarter about *this* repo over time without retraining. Pairs with R3: blast-radius zones populate Provenance fields directly.
**Downsides:** Co-change history requires git log scanning (cheap but not free); outer-zone confidence is inherently noisy. Auto-prune-and-stub mutation of agent prompt is invasive — needs a kill switch. Multi-machine ledger sync is unsolved; per-machine ledger is the v1 default. Overlay file growth needs a vacuum policy.
**Confidence:** 78%
**Complexity:** High
**Status:** Unexplored

## Rejection Summary

| # | Idea | Reason Rejected |
|---|---|---|
| F1.4 | Honest token receipts on every nav call | Subsumed by R2 — typed `Provenance<T>` is the enforced shape of the same rule. |
| F1.5 | Grep-to-AST upgrade adapter wrapping pi's grep | Transitional; R0 replaces grep entirely with R3's `query_semantic_symbols` + non-symbol .scm packs. |
| F1.7 | Edit-aware degradation TUI diagnostic | Subsumed by R5's blast-radius invalidation; same signal, stronger response. |
| F2.1 | Map-first protocol that forbids raw reads | Premature behavioral enforcement; R0 already removes `read(path)` by removing the `grep / find / ls` triad — a positive design replaces a negative protocol. |
| F2.2 | Delete grep, replace with single `xref` | Folded into R0 (replace surface) + R3 (the replacement primitives). |
| F2.3 | Lost-detector server-pushed `reorient` | Replaced by R0's `should_reorient` (Two Corrections Rule, automated). |
| F2.4 | Auto-summarize-on-second-visit (no LLM) | Folded into R5 (the "memo" behavior of the overlay). |
| F2.5 | Remove file paths from agent prompt entirely | Too radical for v1; R2's symbol-path is the equivalent affordance, paths remain available for human-readable logs. |
| F2.7 | Negative-space `unexplored(near=focus_point)` | R3's `missing_symbols` covers the more common error class (typos / renames / deletions); coverage primitive is interesting but secondary. |
| F2.8 | Replace --budget/--quiet/--grammar-path with --profile | Tactical CLI hardening; tracked in `docs/plans/tree-sitter-context-follow-up-plan-2026-04-25.md`. |
| F3.1 | Map = git co-change history projected onto AST | One lens among many; appears as R5's outer blast-radius zone (co-change as confidence-low signal). |
| F3.3 | `REPO_MAP.toml` curated by humans | Shifts ownership; useful as a future opt-in supplement once R1 graph is stable. |
| F3.5 | No global map — per-task ephemeral focus-cones | Conflicts with the user's pre-built map constraint. |
| F3.6 | Many maps as lenses (call/type/error/IO/coverage/ownership) | Partially absorbed: R4 establishes layered + flow lenses; R3's `.scm` query packs make adding lenses cheap. Full lens UI is downstream UX. |
| F3.7 | Subscription model — agent declares interests; server delivers diffs | Overlaps with R5 auto-prune; subscription generalization is a later capability layer. |
| F3.8 | Cross-repo dependency-edge map (FFI/IPC/HTTP/SQL boundaries) | Scope explosion for v1; cross-repo is non-goal of `crates/context` v1. |
| F4.2 | Content-addressed AST cache `~/.cache/pi-mono/ast/<hash>` | Implementation detail; lands naturally with R1's SQLite + XXH3 hashing. |
| F5.1 | Subway/Beck-diagram architecture map | Visualization layer; the structural reframe (concerns as "lines") is partially captured by R4 strata. Visual rendering belongs in TUI design, not core primitive. |
| F5.2 | Wikipedia "What links here" + disambiguation pages | Real primitive but narrow; falls out of R3's `references` query pack + `missing_symbols`. |
| F5.3 | Fog-of-war minimap with persistent discovered tiles | Visualization layer of R5 overlay; core memory is the substrate, fog-of-war is the UI. |
| F5.4 | Score-reduction view (signatures only, bodies collapsed) | R3's `tier ∈ {id|sig|sig+doc|full}` parameter already provides this; lands as `tier=sig`. |
| F5.5 | ATC ground vs en-route mode-typed nav | Special case of R4's lens reframe; mode-as-API is too prescriptive for v1. |
| F5.6 | Co-citation clusters (Garfield 1955) | Compute-heavy and requires usage data; future R3 primitive once R1 is stable. |
| F6.1 | Zero-budget map: delta journal vs LLM prior | Speculative — relies on what Anthropic indexed; unverifiable; conflicts with budget honesty. |
| F6.2 | Million-file mode: hierarchical embedding descent | Embedding training/clustering is out of scope for tree-sitter integration; pushes toward a different subject. |
| F6.3 | Precomputed-planet hosted index | Subject-replacement risk — moves `crates/context` from primitive library to hosted service; explicitly excluded by RFC. |
| F6.4 | Write-only map (trace events only, no AST read) | Subsumed by R5 overlay; "no AST read" is a constraint we don't actually want — combining trace + AST is stronger. |
| F6.5 | Diffuse 100-focus mode (weighted halo) | Conflicts with R0 sticky cursor / focus-handle pattern; brainstorm variant on R3. |
| F6.6 | Mermaid-as-source-of-truth, CI-verified | Contrarian and high friction; humans-maintaining-the-map is a substantial behavior-change ask; brainstorm variant for a later layer. |
| F6.8 | Cross-repo personal memory (`(repo_id, chunk_id)` namespacing) | Scope expansion; valuable later but depends on R5 stabilizing locally first. |

## Cross-Reference

- **Sibling ideation**: [`2026-04-26-tree-sitter-pi-integration-ideation.md`](./2026-04-26-tree-sitter-pi-integration-ideation.md) — the broader earlier survey of integration angles. Several survivors of this doc subsume or replace ideas surveyed there:
  - Earlier Idea #1 (AST-Aware Read Tool) → subsumed by R3 `get_context_bundle` + R0 tool-surface replacement.
  - Earlier Idea #2 (Semantic Session Compaction) → R0's deterministic graph-aware compaction (the Auto-compact replacement).
  - Earlier Idea #4 (Repo Map / Symbol Graph) → R1 (build/update) + R3 (`get_ranked_architecture`).
  - Earlier Idea #5 (Edit Validation) → R3 `safe_edit` (tree-sitter pre-write validation).
  - Earlier Idea #6 (Token-Budgeted Interactive Display) → falls out of R0 + R3 budget primitives; TUI is downstream.
- **Active prototype to harden**: [`docs/plans/tree-sitter-context-rfc-2026-04-24.md`](../plans/tree-sitter-context-rfc-2026-04-24.md) and [`docs/plans/tree-sitter-context-hardening-implementation-plan-2026-04-25.md`](../plans/tree-sitter-context-hardening-implementation-plan-2026-04-25.md). R2 is blocked on `StableId` disambiguation (P1 in the hardening plan). R0/R3 budget primitives are blocked on `estimated_tokens` honesty. R5's invalidation envelope is blocked on per-chunk reason/strategy/confidence in invalidation.
- **Past learnings checkpoint**: [`docs/solutions/workflow-issues/tree-sitter-context-branch-review-2026-04-25.md`](../solutions/workflow-issues/tree-sitter-context-branch-review-2026-04-25.md) — records the design rule R0–R5 all enforce ("every primitive must surface reason/strategy/confidence/omissions").
