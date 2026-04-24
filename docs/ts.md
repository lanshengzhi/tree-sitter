<!-- /autoplan restore point: /home/lansy/.gstack/projects/lanshengzhi-tree-sitter/rfc-tree-sitter-context-autoplan-restore-20260424-221541.md -->

---
title: "RFC: tree-sitter-context"
created: 2026-04-24
status: draft
---

# RFC: tree-sitter-context

`tree-sitter-context` 是一个面向 LLM coding agents 的底层代码上下文引擎。

它不做完整 agent，不做 code review 产品，也不在第一版提供 MCP daemon。它只负责一件事：把源码通过 tree-sitter 变成小、准、可增量更新的上下文单元，让 Aider、Graphify、Code Review Graph、Claude Code、Cursor、Codex 这类上层工具少读无关代码。

## 背景

当前 coding agents 的主要上下文浪费来自三类行为：

1. 直接读取整个文件。
2. 用 `grep` / `rg` 找到关键词后，再把大段源码塞进 prompt。
3. 代码变更后重新读取大量未变化内容。

这会造成两个直接问题：

- token 成本高。
- 模型注意力被无关 imports、注释、样板代码、未修改函数体稀释。

已有工具已经证明这条路有价值：

- Aider 用 repo map 给 LLM 提供文件和 symbol 级摘要。
- Graphify 用 tree-sitter + graph retrieval 做通用知识图谱。
- Code Review Graph 用 tree-sitter + SQLite graph + MCP 做 code review context。

所以 `tree-sitter-context` 不应该再做一个 MCP code graph 产品。那个位置已经有人占了。

真正空出来的位置是更底层的 Rust 引擎：给这些上层工具复用的 semantic context primitives。

## 目标

第一版目标：

1. 用 tree-sitter 在 AST 边界上切分代码。
2. 提取 definitions、references、docs、local scopes 等 symbol 信息。
3. 把 `Tree::changed_ranges` 映射到受影响的语义 chunk。
4. 在 token budget 内输出 compact context bundle。
5. 用 benchmark 证明它比 raw source 和 Aider-style repo map 至少在一个维度上明显更好。

成功标准：

| 指标 | 第一版目标 |
| --- | --- |
| Token reduction | Rust 源码场景比 raw source 少 40%+ tokens |
| Incremental latency | 单文件变更后 affected chunks 计算 < 10ms |
| Retrieval recall | `tree-sitter` 本仓库 20 个定位问题 recall >= 85% |
| Output usability | LLM 能用输出回答定位/影响范围问题，不只是压缩文本 |
| DX | `cargo install` 后 5 分钟内跑通一个有用命令 |

## 非目标

第一版不做：

- MCP server。
- SQLite 持久图谱。
- 向量搜索。
- PageRank、Louvain、社区发现。
- 自动 patch / edit tool。
- 跨语言类型推断。
- 完整 call graph。
- 安全产品包装。
- 夸张的“百倍 token reduction”宣传。

这些以后可以作为 adapter 或上层产品做。核心 crate 不应该背这些复杂度。

## 现有代码杠杆

这个仓库已经有大部分底层能力。

| 能力 | 现有位置 | 用法 |
| --- | --- | --- |
| Incremental parse | `lib/binding_rust/lib.rs` | `Parser::parse(source, Some(&old_tree))` |
| Changed ranges | `lib/binding_rust/lib.rs` | `Tree::changed_ranges(&new_tree)` |
| AST traversal | `lib/binding_rust/lib.rs` | `TreeCursor` / `Node` traversal |
| Query matching | `lib/binding_rust/lib.rs` | `Query` / `QueryCursor` |
| Tags extraction | `crates/tags/src/tags.rs` | definitions, references, docs, local scopes |
| Language loading | `crates/loader/src/loader.rs` | `tree-sitter.json`, `queries/tags.scm`, `queries/locals.scm` |
| CLI patterns | `crates/cli/src/main.rs` | `query`, `tags`, `parse` command structure |
| Changed-range tests | `crates/cli/src/fuzz/corpus_test.rs` | validating changed range behavior |

第一版应该复用这些能力，而不是重写 parser、loader、query 或 tags 层。

## 设计

### 架构

```text
tree-sitter core APIs
        |
        v
tree-sitter-context-core
  - parse cache
  - AST chunker
  - symbol extractor
  - changed-range mapper
  - compact serializer
        |
        v
CLI
        |
        v
future adapters
  - MCP server
  - editor integration
  - code review tools
```

第一版只实现 `tree-sitter-context-core` 和 CLI。

### Crate Layout

建议新增 workspace crate：

```text
crates/context/
  Cargo.toml
  src/
    lib.rs
    chunk.rs
    symbols.rs
    diff.rs
    bundle.rs
    serialize.rs
    token.rs
    error.rs
```

可选 CLI 集成：

```text
crates/cli/src/context.rs
```

如果不想在主 CLI 里加实验命令，可以先单独发 `tree-sitter-context` binary。

### Core Types

```rust
pub struct ContextEngine {
    loader: LanguageLoader,
    token_counter: TokenCounter,
}

pub struct SourceDocument {
    pub path: PathBuf,
    pub language_scope: String,
    pub source: Vec<u8>,
    pub tree: Tree,
}

pub struct CodeChunk {
    pub id: ChunkId,
    pub path: PathBuf,
    pub kind: String,
    pub name: Option<String>,
    pub byte_range: Range<usize>,
    pub point_range: Range<Point>,
    pub parent: Option<ChunkId>,
    pub symbols: Vec<SymbolRef>,
    pub estimated_tokens: usize,
}

pub struct Symbol {
    pub id: SymbolId,
    pub name: String,
    pub kind: SymbolKind,
    pub path: PathBuf,
    pub byte_range: Range<usize>,
    pub docs: Option<String>,
}

pub struct ContextBundle {
    pub target: ContextTarget,
    pub budget: usize,
    pub chunks: Vec<CodeChunk>,
    pub symbols: Vec<Symbol>,
    pub omitted: Vec<OmittedContext>,
}
```

### API

```rust
impl ContextEngine {
    pub fn chunks_for_file(
        &mut self,
        path: &Path,
        source: &[u8],
        options: ChunkOptions,
    ) -> Result<Vec<CodeChunk>>;

    pub fn symbols_for_file(
        &mut self,
        path: &Path,
        source: &[u8],
    ) -> Result<Vec<Symbol>>;

    pub fn changed_chunks(
        &mut self,
        old: &SourceDocument,
        new_source: &[u8],
        edit: InputEdit,
    ) -> Result<Vec<CodeChunk>>;

    pub fn bundle_for_range(
        &mut self,
        path: &Path,
        range: Range<usize>,
        budget: usize,
    ) -> Result<ContextBundle>;
}
```

### CLI

MVP commands：

```bash
ts-context chunks crates/tags/src/tags.rs --budget 2000
ts-context symbols crates/tags/src/tags.rs
ts-context diff old.rs new.rs
ts-context bundle crates/tags/src/tags.rs:120:180 --budget 4000
```

Expected output should be compact but readable:

```scheme
(file "crates/tags/src/tags.rs"
  (chunk function "generate_tags" lines 283..344 tokens 612
    (defines "TagsContext::generate_tags")
    (refs "QueryCursor" "TagsConfiguration" "LocalScope"))
  (chunk struct "TagsConfiguration" lines 28..44 tokens 180))
```

Do not optimize the first version around perfect S-expression design. Make it stable, testable, and easy for models to read.

## Semantic Chunking

Chunking rules should be language-aware but not type-aware.

Priority order:

1. Function / method / class / struct / enum / trait / module boundaries.
2. Query-based definitions from `tags.scm`.
3. Fallback to syntactic top-level nodes.
4. Final fallback to byte/line ranges when no grammar or query exists.

Chunker behavior:

- Never split inside a syntactically complete small function.
- Split large functions by statement/block boundaries only if they exceed budget.
- Preserve parent relationship, for example method belongs to impl/class.
- Attach adjacent docs when tree-sitter tags expose `@doc`.
- Mark parse errors, do not silently hide them.

## Changed-Range Mapping

Tree-sitter already exposes changed byte ranges between old and new trees.

The context layer should map those ranges upward to the smallest meaningful chunks:

```text
old tree + edit + new source
        |
        v
new tree
        |
        v
changed_ranges
        |
        v
affected AST nodes
        |
        v
affected chunks
```

This is the first major differentiation from existing graph tools.

Graph tools often say “these files changed.” `tree-sitter-context` should say “this method body changed, this signature did not, these three parent chunks are affected.”

## Symbol Extraction

First version should reuse `tree-sitter-tags`.

Minimum symbol set:

- definitions
- references
- docs
- local scope boundaries

Do not promise precise cross-file symbol resolution in v1. Without type checking, that promise is fake for many languages.

Instead, expose confidence:

```rust
pub enum ResolutionConfidence {
    Exact,
    QueryMatch,
    NameOnly,
    Unknown,
}
```

## Token Budgeting

The budgeter should be boring.

First version:

- Estimate tokens with a cheap tokenizer or conservative byte heuristic.
- Pack target chunk first.
- Add direct parent chunk summary.
- Add sibling signatures.
- Add referenced symbol signatures.
- Stop before exceeding budget.
- Record omitted context.

Example omitted output:

```scheme
(omitted
  (chunk "parse_query" reason "budget")
  (references 12 reason "max_reference_count"))
```

This matters because models need to know what they did not see.

## Benchmark Plan

Do this before adding MCP, graph ranking, or persistent storage.

### Baselines

Compare against:

1. Raw source file reads.
2. Aider-style repo map.
3. Graphify, if locally runnable.
4. Code Review Graph, if locally runnable.

### Repositories

Start with:

- `tree-sitter/tree-sitter`
- one medium Rust repo
- one TypeScript repo

### Tasks

Use fixed prompts:

1. “Where is symbol X defined?”
2. “What code must I read before editing this function?”
3. “What changed semantically between these two versions?”
4. “Which nearby definitions are likely affected?”
5. “Explain this module’s public API.”

### Metrics

| Metric | Meaning |
| --- | --- |
| Input tokens | How much context was sent |
| Retrieval latency | Time to produce context |
| Recall | Whether required code was included |
| Precision | Whether included code was relevant |
| LLM answer quality | Whether model answered correctly |
| Edit success | Whether model produced correct patch when asked |

### First Benchmark Gate

Continue only if `tree-sitter-context` wins at least one clear dimension:

- materially fewer tokens at same recall,
- faster incremental update,
- better changed-range precision,
- or better edit success for equal token budget.

If it does not win, stop. The world does not need another code context tool that feels clever but does not improve a real workflow.

## Milestones

### Milestone 1: Schema + diagnostics

- Define canonical JSON schema for all context output.
- Add diagnostics and confidence metadata to every response.
- Resource limits and explicit error reporting.
- CLI: `ts-context` outputs structured JSON with diagnostics by default.
- Tests: schema round-trip, diagnostic presence, error shape.

### Milestone 2: Stable chunk identity

- Design an identity scheme that survives renames, moves, and minor edits across runs.
- Identity must be deterministic, comparable, and cache-friendly.
- CLI: identity exposed in schema output.
- Tests: stability across parse runs, stability across edits, collision resistance.

### Milestone 3: Invalidation for old/new snapshots and editor edit streams

- Accept old/new file pair OR `InputEdit` sequence.
- Use incremental parse and `changed_ranges`.
- Map changed ranges to affected chunks.
- Split snapshot diff from edit-stream invalidation (different confidence levels).
- CLI: `ts-context invalidate` (with `diff` as optional alias).
- Tests: body-only change, signature change, doc-only change, whitespace-only change, edit-sequence correctness.

### Milestone 4: Smoke benchmark

- Crude but real benchmark with a few Rust fixtures.
- Measure parse time, changed-range mapping, symbol query, serialization size, token estimate, and end-to-end CLI time.
- Compare against raw source baseline.
- Produce markdown report.
- Gate: must show workflow value before proceeding to generalization work.

### Milestone 5: Chunks/symbols generalization

- Parse one Rust file.
- Emit top-level chunks with names, ranges, token estimates.
- Load `tags.scm` and `locals.scm`, emit definitions/references/docs.
- CLI: `ts-context chunks`, `ts-context symbols`.
- Tests using fixture Rust files and existing tags fixtures where possible.

### Milestone 6: Budgeted bundle

- Given file range or symbol, return packed context under budget.
- Include omitted-context metadata.
- CLI: `ts-context bundle`.

## Risks

| Risk | Impact | Mitigation |
| --- | --- | --- |
| Existing tools already solve enough | Project has no clear reason to exist | Benchmark before product work |
| Cross-file resolution becomes type inference | Scope explodes | v1 exposes confidence, does not promise exactness |
| S-expression bikeshedding | Time wasted on format | Keep v1 output simple and testable |
| MCP distracts from core | Builds product wrapper before engine works | Defer MCP until benchmark passes |
| Grammar query variance | Some languages lack useful tags | Start with Rust, then add Python/TypeScript |
| Token estimate mismatch | Bundle may exceed real model budget | Use conservative estimates and report measured tokens in benchmark |

## Open Questions

1. Should this live inside `tree-sitter/tree-sitter`, or start as a separate repo that depends on tree-sitter crates?
2. Should CLI be integrated into `tree-sitter` or shipped as `ts-context`?
3. Which tokenizer should be used for benchmark measurements?
4. What is the smallest useful output schema for agents?
5. How much should v1 rely on `tree-sitter-tags` versus direct query definitions?

## Recommendation

Start outside the core CLI unless maintainers explicitly want it in-tree.

The fastest useful path is:

```text
crates/context prototype
  -> Rust chunks
  -> symbols via tree-sitter-tags
  -> changed chunks
  -> benchmark report
  -> only then MCP adapter
```

This keeps the project honest. If the benchmark is good, the MCP/product layer is easy. If the benchmark is weak, we learn that quickly without building a 1,000-line daemon nobody should use.

---

## /autoplan Review

### Phase 0: Intake

Plan under review: `tree-sitter-context`, a Rust-first library and CLI for producing token-efficient code context from tree-sitter syntax trees.

Base branch: `master`.

Current branch: `rfc-tree-sitter-context`.

UI scope: no. The plan has no screens, visual components, forms, modals, or user-facing rendering flow.

DX scope: yes. The product is a developer-facing library/CLI for agents and code tools, with explicit API, CLI, install, benchmark, and integration concerns.

Existing design docs: none found for this branch.

Current working tree note: an implementation draft exists in `Cargo.toml` and `crates/context/`, but this review treats it as unapproved draft work. The plan still needs to pass review before implementation continues.

### Phase 1: CEO Review

Mode: SELECTIVE EXPANSION.

#### 0A. Premise Challenge

| Premise | Evaluation | Risk | Recommendation |
| --- | --- | --- | --- |
| Coding agents waste context by reading raw files and broad grep results. | Strong. This is observable in current agent workflows and aligns with Aider/Graphify/Code Review Graph behavior. | Low | Accept. |
| The right response is a low-level Rust context engine, not another MCP code graph product. | Strong. Aider, Graphify, and Code Review Graph already occupy repo-map and MCP graph product space. | Medium | Accept, but keep MCP as a later adapter only after benchmarks. |
| Tree-sitter can provide better semantic chunks than line-based or file-based context. | Strong but needs measurement. Tree-sitter gives syntax boundaries and incremental parse, but chunk usefulness depends on query quality and output format. | Medium | Accept with benchmark gate. |
| `tree-sitter-tags` is the right first symbol layer. | Strong. It is already in this repo and extracts definitions, references, docs, and local scopes. | Low | Accept. |
| Cross-file symbol resolution should not be promised in v1. | Strong. Without type information, exact resolution is fake for many languages. | Low | Accept. |
| A 40% token reduction and 85% retrieval recall are good first targets. | Plausible but unproven. They are useful enough to force measurement, but may be too optimistic for small files and too weak for large ones. | Medium | Accept as initial targets, refine after first benchmark. |
| The project belongs in this tree-sitter repo as `crates/context`. | Not yet proven. It may be better as an external crate until the benchmark proves broad value. | High | Treat as an open decision. Prototype can happen in-tree on this branch, but the RFC should keep "external repo first" as the default unless maintainers want it in-tree. |

#### Premise Gate

The review should continue only if these premises are the right problem framing:

1. We are building a low-level context engine, not a full MCP product.
2. Benchmarks come before product wrappers.
3. v1 focuses on Rust file chunking, tags integration, changed chunks, and budgeted bundles.
4. Exact cross-file symbol resolution is explicitly out of scope for v1.
5. `crates/context` is acceptable as a prototype location, while final placement remains open.

User confirmed this gate with option `A`.

#### 0B. Existing Code Leverage

| Sub-problem | Existing code to reuse | CEO read |
| --- | --- | --- |
| Parse and incremental parse | `lib/binding_rust/lib.rs`: `Parser`, `Tree`, `Tree::edit`, `Tree::changed_ranges` | Reuse directly. Do not build a new parser/cache abstraction until the diff workflow proves it needs one. |
| Syntax traversal | `lib/binding_rust/lib.rs`: `TreeCursor`, `Node`, `Point`, byte ranges | Enough for Rust chunking and affected-node mapping. |
| Query execution | `lib/binding_rust/lib.rs`: `Query`, `QueryCursor`, match/capture iteration and progress limits | Reuse for tags and locals; avoid a second query layer. |
| Definitions/references/docs/local scopes | `crates/tags/src/tags.rs`: `TagsConfiguration`, `TagsContext`, `generate_tags` | This is the strongest low-risk symbol layer. It already encodes repository conventions. |
| Language configuration | `crates/loader/src/loader.rs`: `LanguageConfiguration`, `tags_config`, `tree-sitter.json` loading | Use it for language detection and query discovery instead of hard-coding Rust-only paths beyond fixtures. |
| CLI shape | `crates/cli/src/main.rs`: existing `parse`, `query`, `tags` command patterns | Reuse argument style and output conventions if integrating with the tree-sitter CLI. |
| Changed-range validation | `crates/cli/src/fuzz/corpus_test.rs`: `check_changed_ranges` | Adapt its validation approach for body-only, signature, doc-only, and whitespace edits. |

#### 0C. Dream State Mapping

```text
CURRENT
  Agents read raw files, broad rg output, or product-specific repo maps.
  Tree-sitter has the primitives, but no agent-facing semantic context contract.

THIS PLAN
  A small Rust-first engine/CLI proves four primitives:
  chunks -> symbols -> semantic changed chunks -> budgeted context bundles.

12-MONTH IDEAL
  Multiple coding tools consume a stable context schema.
  Incremental semantic invalidation is the visible wedge:
  after a patch, the engine can say what syntactic units changed, what symbols are
  nearby, what context was omitted, and how confidence was computed.
  Benchmarks and at least one real adapter prove better task outcomes, not just
  prettier compression.
```

Dream-state delta: the current RFC reaches the right primitive layer, but it under-specifies adoption proof. The 12-month ideal needs one real agent integration, a stable output schema, and benchmark tasks tied to edit success or impact analysis accuracy. Without those, the project can be technically correct and still irrelevant.

#### 0C-bis. Implementation Alternatives

| Approach | Effort | Risk | Pros | Cons | CEO decision |
| --- | --- | --- | --- | --- | --- |
| A. Semantic diff CLI first | S-M | Medium | Makes `Tree::changed_ranges` the first proof point; fast to benchmark; directly differentiates from repo maps. | Delays the broad chunk/symbol crate shape. | User Challenge: both outside voices recommend moving this earlier than the RFC currently does. |
| B. Current RFC sequence: chunks -> symbols -> diff -> bundle -> benchmark | M | Medium | Clean layering; easy to implement in small slices; aligns with existing draft. | Benchmarks and adoption proof come late; may polish the wrong abstraction. | Viable baseline, but needs earlier benchmark and adapter proof. |
| C. MCP/code graph product now | L-XL | High | Directly usable by agents; adoption surface is clearer. | Duplicates existing products, expands storage/protocol scope, distracts from tree-sitter-native edge. | Reject for v1. Defer until primitive benchmark passes. |
| D. Benchmark corpus plus thin adapter first, then extract engine | M | Medium-High | Proves demand and workflow value before architecture hardens. | Less tidy as an in-repo Rust crate; may require tool-specific glue. | Taste decision: recommended as a validation track alongside the engine, not a replacement yet. |

#### 0D. Selective Expansion Analysis

| Candidate | Decision | Classification | Reason |
| --- | --- | --- | --- |
| Move semantic changed chunks from Milestone 3 to the first proof point. | Surface at final gate. | User Challenge | Claude and Codex both say the differentiator is incremental semantic invalidation, not generic token reduction. This changes the user's milestone order. |
| Add a benchmark smoke test by Milestone 2, before bundle work. | Add. | Mechanical | In blast radius, small effort, prevents building a polished abstraction without evidence. |
| Tighten the benchmark gate from "wins one dimension" to "wins a real workflow dimension." | Add. | Mechanical | A single micro-win is not enough; the plan should require equal-or-better task success with lower cost, or materially better impact analysis. |
| Add one real adapter spike after the first useful CLI. | Defer to final gate as taste. | Taste | It may be the adoption proof the plan needs, but it can pull scope toward product work. |
| Expand first language scope beyond Rust. | Defer. | Mechanical | Useful later, but outside the <1 day blast radius and weakens focus. |
| Specify a stable JSON schema earlier. | Add. | Mechanical | For a library/CLI primitive, output schema is the product contract. |
| Build MCP server in v1. | Reject. | Mechanical | Duplicates existing products and contradicts the accepted premise. |

Accepted scope adjustments for the remaining review:

1. Add benchmark smoke tests before bundle work.
2. Treat output schema stability as a v1 deliverable, not bikeshedding.
3. Strengthen the benchmark gate to workflow value, not only compression.

Deferred or challenged scope remains listed under "NOT in scope" and the final gate.

#### 0E. Temporal Interrogation

| Time horizon | Likely failure | Rescue |
| --- | --- | --- |
| Hour 1 | Start implementing chunk traversal without a measurable task. | Define the first benchmark fixture and expected answer before writing more engine code. |
| Hour 6 | `crates/context` grows a tidy API but no proof an agent can consume it. | Add CLI output snapshots and a thin adapter spike plan before expanding modules. |
| Day 2 | Symbol extraction quality depends on tags queries that vary by language. | Keep Rust fixtures first; report confidence and missing query capabilities explicitly. |
| Week 1 | Benchmark passes on token reduction but not task quality. | Stop or pivot. The gate must require workflow value at equivalent or lower token budget. |
| Month 1 | In-tree placement creates maintenance debate before value is proven. | Keep final placement open; treat in-tree work as prototype evidence only. |
| Six months | The project becomes a clean Rust library nobody routes agent workflows through. | Prioritize semantic diff plus adapter validation and publish evidence from real tasks. |

#### 0F. Mode Selection Confirmation

SELECTIVE EXPANSION remains correct. The RFC has a coherent core and strong local code leverage, so scope reduction would throw away real opportunity. Full scope expansion into MCP, graph storage, or multi-language indexing would duplicate existing products. The right move is to keep the primitive layer, add measurement earlier, and surface the one major strategic reframing at the final gate.

#### 0.5. Dual Voices

##### CODEX SAYS (CEO - strategy challenge)

Codex flagged 12 concerns. The highest-severity concerns were:

1. The plan assumes the open market position is a reusable Rust engine, but adoption into real agents may be the scarce asset.
2. The problem is framed as token waste, while the stronger user pain may be failed edits, missing dependency context, and unreliable impact analysis.
3. Deferring all wrappers may be strategically backwards if it prevents proof in a real workflow.
4. The benchmark gate is too easy because "wins one dimension" can pass without improving task outcomes.
5. The tasks are biased toward what AST chunking does well and underweight multi-file edits, generated code, framework behavior, and hidden conventions.
6. Competitive risk is understated: major agent/editor products can copy AST chunking if it matters.
7. Rust-first may be convenient for this repo but not the strongest adoption market.
8. Changed-range precision is syntactic, not semantic impact; the plan must avoid overselling it.
9. The first adopter is undefined.
10. The output schema is not bikeshedding; it is the integration contract.
11. The likely six-month regret is polishing `crates/context` before proving one killer integration.

##### CLAUDE SUBAGENT (CEO - strategic independence)

Claude flagged 10 issues. The strongest points were:

1. Reframe from "token-efficient chunks" to "incremental semantic context invalidation for coding agents."
2. Prove one real adapter or integration friction path before treating the engine as valuable.
3. Move changed-range mapping earlier because it is the strongest tree-sitter-native differentiator.
4. Add a throwaway benchmark by Milestone 2, not after all primitives are built.
5. Strengthen the benchmark gate so it measures end-to-end workflow improvement.
6. Add failure classes for trait impl lookup, macro-heavy Rust, test-to-source navigation, generated bindings, and config-driven behavior.
7. Define the defensible edge as tree-sitter-native incremental semantic diff with stable schemas and query reuse.
8. Treat in-repo placement as risky until value is proven.
9. The six-month regret is building a polished primitive with no pull from real users.

##### CEO Dual Voices - Consensus Table

| Dimension | Claude | Codex | Consensus |
| --- | --- | --- | --- |
| Premises valid? | Mostly valid, but demand/adoption premise unproven. | Mostly valid, but library adoption is assumed. | CONFIRMED concern: add adoption proof. |
| Right problem to solve? | Reframe to semantic context invalidation. | Reframe from token waste to edit reliability and impact analysis. | CONFIRMED User Challenge. |
| Scope calibration correct? | Core scope okay, but changed chunks and benchmark should move earlier. | Core scope okay, but wrappers/adapters may be needed for proof. | PARTIAL: keep primitive scope, challenge milestone order. |
| Alternatives sufficiently explored? | No; adapter-first and benchmark-first options need more weight. | No; benchmark corpus plus thin adapter may be better first move. | CONFIRMED gap. |
| Competitive/market risks covered? | Understated; incumbents can copy chunking. | Understated; distribution and evals are stronger moats. | CONFIRMED gap. |
| 6-month trajectory sound? | Risk of polished primitive with no pull. | Risk of clean infrastructure before demand proof. | CONFIRMED concern. |

Consensus result: 5/6 confirmed concerns and 1 partial agreement. Two items are User Challenges for the final gate: reframe the wedge around incremental semantic invalidation, and reorder the MVP so semantic diff plus benchmark evidence arrives before generic bundle work.

#### Section 1: Architecture Review

Examined the proposed crate layout, API, data flow, and existing code leverage. The architecture is directionally sound if it stays a thin layer over parser, loader, tags, and query APIs. The main issue is sequencing: `ContextEngine` could become a premature abstraction if the first implemented workflow is generic chunking instead of semantic diff evidence.

Decision: keep the small module split only after the first CLI proves which modules are real. Prefer `diff`, `chunk`, and `serialize` slices tied to fixtures over a broad engine facade on day one.

#### Section 2: Error & Rescue Map

| Error path | Trigger | User-visible failure | Rescue |
| --- | --- | --- | --- |
| Language not detected | Unknown extension or missing `tree-sitter.json` config | CLI cannot parse file | Return explicit unsupported-language error with path and expected config source. |
| Tags query missing | Grammar lacks `tags.scm` or `locals.scm` | Symbols absent or misleading | Emit chunks with `symbols_confidence = none`; do not fail chunking. |
| Query capture mismatch | Query uses unsupported or unexpected capture names | Definitions/references incomplete | Validate capture names and include diagnostics in debug output. |
| Incremental edit invalid | Caller passes wrong `InputEdit` | Changed chunks incorrect | Provide old/new full-parse fallback and test edit mapping against changed ranges. |
| Token estimate mismatch | Heuristic estimate differs from target tokenizer | Bundle exceeds real budget | Use conservative estimate and include measured-token benchmark mode. |
| Output schema drift | CLI and library evolve independently | Adapters break | Snapshot JSON outputs and version the schema. |
| Parse error nodes | Source has syntax errors | Chunk boundaries may be unstable | Surface parse-error ranges and confidence instead of hiding them. |

#### Section 3: Security & Threat Model

No high-risk security feature is introduced by the RFC itself: v1 reads local source files, parses them, and emits structured context. The main risks are resource exhaustion from large files or pathological queries, accidental disclosure if an adapter later sends omitted-but-sensitive context, and unsafe assumptions if output is treated as authoritative.

Auto-decisions: add parser/query timeout or progress limits where available, cap file size for CLI defaults, and require adapters to make transmission boundaries explicit. Do not add a security product wrapper in v1.

#### Section 4: Data Flow & Interaction Edge Cases

```text
path + bytes
  -> loader selects language and queries
  -> parser builds Tree
  -> chunk traversal emits ranges
  -> tags query attaches symbols/docs/local scopes
  -> changed_ranges maps old/new trees to affected chunks
  -> serializer emits stable schema with omissions and confidence
```

Edge cases that must be tested: empty files, parse errors, macro-heavy Rust, impl blocks, nested modules, doc-only changes, whitespace-only changes, signature-only changes, body-only changes, generated files, and paths whose language cannot be loaded. The plan already covers some changed-range cases; it should add macro/impl/generated-code failure classes because these are where coding agents lose trust.

#### Section 5: Code Quality Review

The plan's code quality risk is over-abstraction. A single `ContextEngine` with loader, token counter, parse cache, chunker, symbol extractor, diff mapper, bundle packer, and serializer can hide too much before behavior is proven. The cleanest path is fixture-driven, with public types introduced only when a CLI or benchmark snapshot needs them.

Decision: keep types explicit and serializable, but avoid building a configurable framework until after the first benchmark report. Reuse existing error types and query APIs where practical.

#### Section 6: Test Review

```text
Unit fixtures
  Rust chunk boundaries
  symbol/doc extraction
  changed chunk mapping
  serializer snapshots

Integration fixtures
  ts-context chunks
  ts-context symbols
  ts-context diff
  benchmark smoke report

Workflow fixtures
  locate symbol
  explain impact of changed method body
  test-to-source navigation
  macro/impl-heavy file
```

Coverage gap: the original milestones delay benchmark testing until Milestone 5. CEO review changes that: a benchmark smoke test must exist by Milestone 2, even if it is crude and uses only a few Rust fixtures.

#### Section 7: Performance Review

Performance targets are plausible because tree-sitter incremental parse and changed ranges are already efficient. The risk is measuring only engine latency while ignoring adapter and serialization cost. The first benchmark should separately report parse time, changed-range mapping time, symbol query time, serialization size, token estimate, and end-to-end CLI time.

Decision: keep `< 10ms` affected-chunk computation as a target, but require hardware and corpus notes in the benchmark report.

#### Section 8: Observability & Debuggability Review

The plan needs debug output as a first-class DX feature. Developers integrating this will need to see why a chunk was included, why another was omitted, which query captures matched, and which changed range caused an affected chunk.

Decision: add `--debug-context` or equivalent structured diagnostics before any adapter work. Keep default output compact, but make the reasoning inspectable.

#### Section 9: Deployment & Rollout Review

Rollout risk is mostly placement and packaging. In-tree `crates/context` gives fast access to internal APIs but may create a maintenance and governance burden. External crate gives cleaner adoption but loses some distribution advantages.

Decision: prototype in-tree is acceptable on this branch, but the RFC should continue to recommend external-first unless maintainers explicitly want the crate in tree. Do not publish or stabilize names until benchmark and schema snapshots exist.

#### Section 10: Long-Term Trajectory Review

Reversibility: 4/5 if kept as a prototype crate/CLI with snapshot tests and no stable API promise. Reversibility drops to 2/5 if the tree-sitter CLI exposes a stable command too early.

Long-term debt to avoid: pretending syntactic changed ranges equal semantic impact, growing a bespoke graph store, coupling to one tokenizer too early, and designing an output format that only this CLI can consume.

Section 11 skipped: no UI scope was detected.

#### NOT in scope

| Item | Rationale |
| --- | --- |
| MCP server in v1 | Duplicates existing code graph products and should wait for benchmark proof. |
| SQLite graph store | Adds persistence and graph modeling before the primitive is validated. |
| Vector retrieval | Orthogonal to tree-sitter-native changed-range differentiation. |
| Exact cross-file type/symbol resolution | Requires language semantics beyond tree-sitter queries. |
| Multi-language support beyond Rust in first proof | Valuable later, but weakens the first benchmark loop. |
| Security/code-review product wrapper | This RFC is about context primitives, not an end-user review product. |
| Stable public API guarantee before benchmarks | Would harden the wrong contract too early. |

#### What Already Exists

The repository already provides parser bindings, incremental parse/edit APIs, changed ranges, AST traversal, query execution, tags extraction, language loading, CLI command patterns, and changed-range validation patterns. The new work should mostly compose these pieces into an agent-facing context contract and benchmark harness.

#### Error & Rescue Registry

| # | Error | Detection | Rescue |
| --- | --- | --- | --- |
| E1 | Unsupported language | Loader cannot resolve config | Return explicit unsupported-language diagnostic. |
| E2 | Missing tags query | `tags_config` unavailable | Emit chunks without symbols and mark confidence. |
| E3 | Query mismatch | Capture validation fails | Warn/debug with query file and capture name. |
| E4 | Bad incremental edit | Changed ranges implausible or tests fail | Fall back to full parse and report degraded mode. |
| E5 | Token budget overflow | Serializer exceeds budget | Conservative estimate, measured benchmark, omitted metadata. |
| E6 | Schema drift | Snapshot diff changes | Version schema and gate changes in tests. |

#### Failure Modes Registry

| # | Failure mode | Severity | Mitigation |
| --- | --- | --- | --- |
| F1 | Token reduction improves but task success does not. | Critical | Strengthen benchmark gate to real workflow outcomes. |
| F2 | Engine API is polished before adoption is proven. | Critical | Add early benchmark and adapter validation. |
| F3 | Changed ranges are mistaken for semantic impact. | High | Label confidence and benchmark impact-analysis tasks. |
| F4 | Grammar query variance makes symbols unreliable. | High | Report missing/low-confidence symbols instead of hiding gaps. |
| F5 | Output schema is inconvenient for tools. | High | Treat schema snapshots and docs as v1 deliverables. |
| F6 | In-tree placement blocks acceptance. | Medium | Keep external-first recommendation until maintainers opt in. |
| F7 | Large files or pathological queries cause slow CLI runs. | Medium | Add limits, progress controls, and performance reporting. |

#### CEO Completion Summary

| Review area | Result |
| --- | --- |
| 0A Premises | 6 accepted, 1 open placement risk |
| 0B Existing leverage | Strong: parser, changed ranges, tags, loader, CLI all reusable |
| 0C Dream state | Needs adoption proof and stable schema to reach 12-month ideal |
| 0C-bis Alternatives | 4 approaches considered; MCP product rejected; semantic-diff-first challenged |
| 0D Scope | 3 additions accepted, 2 user/taste challenges surfaced, 2 expansions deferred/rejected |
| Section 1 Architecture | 1 issue: premature broad engine facade |
| Section 2 Errors | 7 error paths mapped |
| Section 3 Security | 0 high-risk issues; resource/disclosure boundaries noted |
| Section 4 Data/edge cases | 10 edge cases mapped, macro/impl/generated cases added |
| Section 5 Quality | 1 issue: over-abstraction risk |
| Section 6 Tests | 1 gap: benchmark smoke test too late |
| Section 7 Performance | 1 issue: measure end-to-end, not only engine latency |
| Section 8 Observability | 1 gap: debug reasoning output needed |
| Section 9 Rollout | 1 risk: in-tree vs external placement |
| Section 10 Future | Reversibility 4/5 if kept experimental |
| Section 11 Design | SKIPPED: no UI scope |

Phase 1 complete. Codex: 12 concerns. Claude subagent: 10 issues. Consensus: 5/6 confirmed concerns, 1 partial agreement, 2 user challenges to surface at the final gate.

### Phase 2: Design Review

Skipped: no UI scope. The plan has no screens, visual components, layout, interaction states, or user-visible rendering behavior. Design Review outputs are therefore not applicable.

### Phase 3: Engineering Review

Mode: FULL_REVIEW via `/autoplan`, with all intermediate decisions auto-decided by the six decision principles except User Challenges.

#### Step 0. Scope Challenge

Actual code reviewed: `lib/binding_rust/lib.rs`, `crates/tags/src/tags.rs`, `crates/loader/src/loader.rs`, `crates/cli/src/main.rs`, `crates/cli/src/fuzz/corpus_test.rs`, `crates/cli/src/tests/tree_test.rs`, and the unapproved draft `crates/context/`.

Scope challenge result: do not reduce the primitive scope, but block implementation until the plan is rewritten around invalidation-first proof. The minimum complete implementation still needs chunks, symbols, diff/invalidation, schema, diagnostics, resource limits, benchmark smoke tests, and CLI output snapshots. The risky shortcut is building generic chunking first and hoping diff/bundles prove value later.

Complexity check: the proposed crate layout touches more than eight modules if built literally. That is acceptable only if the first vertical slice is narrow. The reviewed execution order should be: invalidation smoke test and schema snapshot first, then chunk/symbol generalization, then bundles.

Distribution check: a new binary/library artifact is proposed. Build/publish pipeline is not in v1 scope; the plan must state that local `cargo` usage is the only distribution target until benchmarks pass.

#### 0.5. Dual Voices

##### CODEX SAYS (eng - architecture challenge)

Codex flagged 9 issues:

1. Critical: generic chunking-first / benchmark-last sequencing should be blocked; start with `diff` / `invalidate` plus smoke benchmark.
2. Critical: `changed_chunks(old, new_source, edit)` hides valid edit construction; old/new snapshots and editor edit streams need separate API modes.
3. Critical: the plan says "semantic" for syntactic changed ranges; invalidation must union raw edited byte ranges, syntax changed ranges, and optional symbol/doc deltas.
4. High: chunk identity is under-specified for incremental invalidation; run-local ids are insufficient.
5. High: `ContextEngine { loader, token_counter }` couples core and CLI/runtime discovery too early.
6. High: referenced symbol signatures are not available without an index or resolver.
7. High: output format is contradictory; JSON schema must be canonical and S-expression should be display-only.
8. Medium: benchmark gates are too easy and need negative cases.
9. Medium: resource-control defaults are missing despite existing tags cancellation support.

##### CLAUDE SUBAGENT (eng - independent review)

Claude flagged 9 issues:

1. High: `ContextEngine` is too stateful for the proposed API; cache/session state and stateless operations should be separated.
2. High: changed-range mapping risks overselling syntactic deltas as semantic impact.
3. High: output schema is under-specified despite being the integration contract.
4. Medium: large-file and pathological-query behavior needs explicit bounds.
5. Medium: nil/empty/error paths need first-class API semantics.
6. Medium: test plan misses adversarial and regression fixtures.
7. Medium: local parsing is not zero-risk; sanitize terminal output, cap resources, and define workspace-root behavior for adapters.
8. Medium: token budgeting needs `estimated_tokens` vs `measured_tokens`.
9. Low: in-tree crate placement creates coupling and maintenance ambiguity.

##### Eng Dual Voices - Consensus Table

| Dimension | Claude | Codex | Consensus |
| --- | --- | --- | --- |
| Architecture sound? | Sound direction, but state/cache ownership is risky. | Block until invalidation-first and API modes are explicit. | CONFIRMED gap. |
| Test coverage sufficient? | Missing adversarial/regression fixtures. | Benchmark smoke and negative cases must move earlier. | CONFIRMED gap. |
| Performance risks addressed? | Bounds and degraded output not specified. | Resource controls and partial output missing. | CONFIRMED gap. |
| Security threats covered? | Local but not zero-risk; sanitize/cap/root-bound. | Resource controls needed before arbitrary repos. | CONFIRMED gap. |
| Error paths handled? | Empty/error/degraded states need structured diagnostics. | Edit-stream vs snapshot fallback must be explicit. | CONFIRMED gap. |
| Deployment risk manageable? | Experimental placement keeps it reversible. | Distribution should wait for benchmark proof. | CONFIRMED with constraint. |

Consensus result: 6/6 confirmed concerns. One item is a User Challenge already raised by CEO: reorder the MVP around invalidation-first proof. Several engineering fixes are auto-decided because they are in-blast-radius and necessary for correctness.

#### Section 1: Architecture

```text
tree-sitter parser APIs
  Parser / Tree / InputEdit / changed_ranges
        |
        v
context core
  chunk model
  stable chunk identity
  stateless parse/chunk operations
  invalidation mapper
  diagnostics + confidence
        |
        +--> tags adapter
        |      TagsConfiguration / TagsContext
        |
        +--> serializer
        |      canonical JSON schema
        |      optional human display
        |
        +--> benchmark smoke harness
        |
        v
CLI wrapper
  chunks
  symbols
  diff/invalidate
  bundle
```

Findings:

1. [P1] (confidence: 8/10) `docs/ts.md:191` - `changed_chunks(old, new_source, edit)` conflates editor edit-sequence mode with old/new snapshot mode. Decision: split into explicit API modes and mark snapshot diff as degraded confidence.
2. [P1] (confidence: 8/10) `docs/ts.md:159` - `ChunkId` and identity stability are not defined strongly enough for invalidation. Decision: v1 schema must define stable identity inputs such as path, language, node kind, name path, byte/point range, and content hash.
3. [P2] (confidence: 8/10) `docs/ts.md:147` - `ContextEngine` risks coupling core library to loader/tokenizer state too early. Decision: core should accept parsed `Tree`, `Language`, source bytes, and optional `TagsConfiguration`; CLI owns loader discovery.
4. [P2] (confidence: 7/10) `docs/ts.md:320` - referenced symbol signatures require an index/resolver that v1 does not have. Decision: same-file references only unless caller provides an explicit symbol index.

#### Section 2: Code Quality

The primary quality risk is not messy Rust yet; it is an over-broad public API hardened before the benchmark tells us which abstractions matter. The unapproved draft already shows a run-local `ChunkId`, which is fine for simple chunking but incompatible with the invalidation wedge if it becomes the public contract.

Auto-decisions:

1. Keep `crates/context` experimental and avoid public API stability claims.
2. Prefer small pure functions for chunking and invalidation before a stateful engine facade.
3. Make `ContextDiagnostic`, `Confidence`, and `OmittedContext` explicit types, not logging side effects.
4. Canonical output is JSON with snapshots; S-expression/readable output is display-only.

#### Section 3: Test Review

Test framework: Rust workspace with `cargo test`; existing relevant tests include `crates/cli/src/tests/tree_test.rs`, `crates/cli/src/tests/tags_test.rs`, `crates/cli/src/tests/test_tags_test.rs`, and fuzz/corpus changed-range validation helpers.

```text
CODE PATHS
[+] parse/chunk one Rust file
  ├── [GAP] happy path top-level items
  ├── [GAP] nested module / impl / trait / enum boundaries
  ├── [GAP] empty file and parse-error file diagnostics
  └── [GAP] CRLF, UTF-8 multibyte, invalid bytes range stability

[+] symbol attachment via tags
  ├── [★★ TESTED upstream] tags extraction examples exist in tags tests
  ├── [GAP] missing tags config -> chunks still emitted with diagnostics
  └── [GAP] docs/local scopes attached without hiding query gaps

[+] invalidation / diff
  ├── [★★ TESTED upstream] changed_ranges core tests exist
  ├── [GAP] editor edit-sequence mode with valid InputEdit
  ├── [GAP] old/new snapshot mode with degraded confidence
  ├── [GAP] raw edited byte ranges + syntax ranges + symbol/doc deltas union
  ├── [GAP] signature-only, body-only, doc-only, whitespace-only changes
  └── [GAP] macro-heavy Rust, duplicate method names, moved/reordered methods

[+] bundle and token budget
  ├── [GAP] target chunk first, omissions recorded
  ├── [GAP] same-file references only without index
  ├── [GAP] estimated vs measured token reporting
  └── [GAP] serializer overhead included in budget

[+] CLI and schema
  ├── [GAP] canonical JSON schema snapshot
  ├── [GAP] display output snapshot
  ├── [GAP] unsupported language / missing query / parse error diagnostics
  └── [GAP] terminal output sanitization

WORKFLOW / BENCHMARK PATHS
[+] Smoke benchmark before bundle work
  ├── [GAP] raw source baseline
  ├── [GAP] repo-map baseline
  ├── [GAP] invalidation task quality
  └── [GAP] negative cases: trait impl lookup, generated code, test-to-source navigation
```

Coverage: plan-stage only; existing upstream tests cover parser/tags primitives, but new context paths are not implemented yet. Required implementation tests are all gaps and must be added with the feature slices.

Test plan artifact written: `/home/lansy/.gstack/projects/lanshengzhi-tree-sitter/lansy-rfc-tree-sitter-context-eng-review-test-plan-20260424-222832.md`.

#### Section 4: Performance

Findings:

1. [P1] (confidence: 8/10) Resource limits are not specified for arbitrary repositories. Decision: define default max file bytes, max chunks, max query matches, max references per symbol, max serialized bytes, and timeout/cancellation behavior before CLI use on real repos.
2. [P2] (confidence: 8/10) `< 10ms` affected chunk computation is incomplete without query, serialization, and token measurement. Decision: benchmark reports parse, query, invalidation, serialization, and total CLI time separately.
3. [P2] (confidence: 7/10) Token budgeting can undercount JSON/schema overhead. Decision: expose both `estimated_tokens` and benchmark-only `measured_tokens`, with a conservative margin in v1.

#### Phase 3 NOT in scope

| Item | Rationale |
| --- | --- |
| Publish/release pipeline | Distribution waits until benchmark proof; local cargo use is enough for v1 validation. |
| Full cross-file symbol index | Needed for referenced signatures, but outside v1 unless provided by caller. |
| Rename/move semantic detection | Stable identity should make this possible later; v1 only reports confidence and deltas. |
| Remote adapter path security | Future adapters must enforce roots; v1 CLI remains local. |

#### Phase 3 What Already Exists

Parser/edit/changed-range primitives, query execution, tags extraction, loader config, CLI command patterns, cancellation in tags, and changed-range tests already exist. The plan should not rebuild them. New code should add identity, diagnostics, schema, invalidation mapping, and benchmark evidence around those primitives.

#### Phase 3 Failure Modes

| Codepath | Failure mode | Test? | Error handling? | User-visible? | Critical gap |
| --- | --- | --- | --- | --- | --- |
| Edit-sequence invalidation | Caller supplies stale or wrong `InputEdit` | Missing | Missing | Could be silent wrong chunks | Yes |
| Snapshot diff | Old/new files require multiple edits or file move | Missing | Missing | Degraded result unclear | Yes |
| Chunk identity | Duplicate names or reordered methods collide | Missing | Missing | Cache invalidation wrong | Yes |
| Tags attachment | Missing query returns no symbols | Missing | Planned diagnostics | Should be visible | No |
| Token budget | Estimated budget undercounts output overhead | Missing | Planned measured mode | Could exceed model budget | No |
| Large file | Query or serialization runs too long | Missing | Missing limits | Hang or truncation | Yes |
| Terminal output | Source text includes escape sequences | Missing | Missing sanitizer | Confusing/unsafe terminal display | No |

#### Worktree Parallelization Strategy

| Step | Modules touched | Depends on |
| --- | --- | --- |
| Schema + diagnostics | `crates/context`, test snapshots | None |
| Chunk identity + chunking | `crates/context` | Schema + diagnostics |
| Invalidation mapper | `crates/context`, changed-range fixtures | Schema + diagnostics, chunking |
| Tags adapter | `crates/context`, `crates/tags` usage | Schema + diagnostics |
| CLI wrapper | `crates/cli` or standalone binary | Core slices |
| Benchmark smoke harness | `crates/context` or benchmark harness | Invalidation mapper, CLI/schema |

Parallel lanes: Lane A `schema + diagnostics -> chunk identity`; Lane B `tags adapter` after schema; Lane C `benchmark smoke harness` waits for invalidation and CLI/schema. Because most work touches `crates/context`, implementation should be mostly sequential unless split into isolated branches with careful merge order.

#### Phase 3 Completion Summary

| Review area | Result |
| --- | --- |
| Step 0 Scope Challenge | Scope kept, ordering challenged |
| Architecture Review | 4 issues found |
| Code Quality Review | 4 auto-decisions |
| Test Review | Diagram produced, 22 gaps identified |
| Performance Review | 3 issues found |
| NOT in scope | Written, 4 items |
| What already exists | Written |
| Test plan artifact | Written to `.gstack/projects` |
| Failure modes | 4 critical gaps flagged |
| Outside voice | Codex + Claude subagent ran |
| Parallelization | 3 lanes identified, mostly sequential due shared crate |
| Lake Score | 7/7 recommendations chose complete option |

Phase 3 complete. Codex: 9 concerns. Claude subagent: 9 issues. Consensus: 6/6 confirmed concerns, 1 carried User Challenge.

### Phase 3.5: DX Review

Mode: DX POLISH.

Product type: CLI Tool + Rust library/SDK for coding-agent tool builders.

#### Developer Persona Card

| Field | Persona |
| --- | --- |
| Who | Agent/tool builder or tree-sitter ecosystem Rust/CLI developer evaluating whether to integrate this context engine. |
| Context | They already know tree-sitter or coding agents, and want proof that this engine gives better context than raw files, repo maps, or ad hoc `rg`. |
| Tolerance | 2-5 minutes to see useful output; 10+ minutes without proof means they abandon or treat it as research. |
| Expects | One install/build command, one copy-paste demo, machine-readable output, stable schema, actionable diagnostics, and clear limits. |

#### Developer Empathy Narrative

I open the RFC because I want a context primitive for my coding agent. The first page tells me the idea is a low-level engine, which is promising, but I cannot immediately run it. I see `ts-context chunks`, `symbols`, `diff`, and `bundle`, but no literal install path, fixture files, or expected JSON output. I notice the success criteria says `cargo install` in five minutes, while the engineering review says distribution is local-only until benchmarks pass. I am not sure whether I should run `cargo install`, `cargo run -p tree-sitter-context`, or use the existing `tree-sitter` CLI. The most interesting thing is invalidation, but the first milestone is chunking and the first visible command does not prove the differentiated workflow. If I compare this against Aider-style repo maps or an MCP graph tool, I still do not know what I would integrate or how stable the output contract is.

#### Competitive DX Benchmark

| Tool | TTHW | Notable DX choice | Source |
| --- | --- | --- | --- |
| tree-sitter CLI | 2-5 min once dependencies are available | `cargo install --locked tree-sitter-cli` or `npm install tree-sitter-cli`; clear command list | `README.md`, `crates/cli/README.md`, npm tree-sitter-cli |
| Aider repo map | Integrated into existing terminal workflow | Repo map is automatic context inside the agent loop; users do not integrate a separate schema first | Aider docs |
| `tree-sitter-context` current RFC | Not measurable, likely 10-20 min for external evaluator | Commands exist, but no complete quickstart, fixture, output contract, or install decision | Current plan |
| `tree-sitter-context` target | 2-5 min | `cargo install --path crates/context` or `cargo run -p tree-sitter-context -- demo invalidate`, with expected JSON output | DX review target |

Target tier: Competitive, 2-5 minutes. Champion tier is possible later, but competitive is the right v1 bar because the tool is experimental and local-only.

#### Magical Moment Specification

Delivery vehicle: copy-paste terminal demo.

The magical moment is not "here are chunks." It is seeing one command explain a source change with confidence and reasons:

```bash
ts-context invalidate examples/before.rs examples/after.rs --format json --explain
```

The output should visibly show `schema_version`, `mode`, `confidence`, `changed_chunks`, whether a signature changed, why the chunk was included, and diagnostics. This proves workflow value before the developer reads API docs.

#### Developer Journey Map

| Stage | Developer does | Friction | Status |
| --- | --- | --- | --- |
| Discover | Reads RFC or README. | Value prop is mixed between token reduction and invalidation. | Fix: lead with invalidation-first proof. |
| Install | Looks for `cargo install` or local build command. | Plan conflicts between install success metric and local-only distribution. | Fix: v1 quickstart uses local `cargo` explicitly. |
| Hello World | Runs first command. | No fixture, expected output, or `demo invalidate` command. | Fix: add copy-paste invalidation demo. |
| Real Usage | Tries own file pair or editor edit stream. | Snapshot diff and edit-sequence semantics are unclear. | Fix: separate modes and confidence. |
| API Integration | Needs stable machine output. | S-expression vs JSON ambiguity. | Fix: canonical JSON v0 with snapshots. |
| Debug | Something is missing or degraded. | No problem/cause/fix/docs-link diagnostic contract. | Fix: structured diagnostics and `--explain`. |
| Tune | Needs different tokenizer/query/resource limits. | Escape hatches are not specified. | Fix: defaults table plus flags/API options. |
| Upgrade | Schema/CLI changes. | No experimental versioning policy. | Fix: schema version, changelog, deprecation rules. |
| Validate | Compares against raw source/repo map. | Benchmark is too late. | Fix: smoke benchmark before bundle work. |

#### First-Time Developer Confusion Report

```text
Persona: agent/tool builder evaluating integration
Attempting: get useful context invalidation output

T+0:00  I open the RFC and understand the strategic goal, but I do not see a Quickstart section.
T+0:30  I find CLI examples, but they assume a `ts-context` binary that is not installed.
T+1:30  I look for the most differentiated command. `diff` exists, but it is Milestone 3 and its output is not specified.
T+3:00  I look for JSON schema because I would integrate this into a tool. The plan shows S-expression first and JSON only in review notes.
T+5:00  I still cannot run a useful demo or evaluate schema stability. I treat the project as an internal RFC, not integration-ready.
```

#### 0.5. Dual Voices

##### CODEX SAYS (DX - developer experience challenge)

Codex scored current DX at 4/10 and flagged 6 blockers:

1. TTHW fails the under-5-minute target because no install command, fixture, expected output, or copy-paste demo exists.
2. Error-message DX is planned but not designed; registries do not replace actual stderr, exit codes, and degraded-mode JSON.
3. CLI/API naming is not guessable: product name, binary name, `diff`, `invalidate`, and `changed_chunks` compete.
4. Output contract is ambiguous; JSON schema must be the product and S-expression display-only.
5. Docs are not findable in under 2 minutes because quickstart, CLI reference, Rust API, JSON schema, troubleshooting, benchmarks, and limitations are not separated.
6. Upgrade path is absent; even an experimental prototype needs schema versioning, changelog, deprecation rules, and migration notes once external users exist.

##### CLAUDE SUBAGENT (DX - independent review)

Claude found 8 issues:

1. High: hello-world path is missing and the install story conflicts.
2. High: CLI naming is not settled enough for users or docs.
3. Critical: error handling does not meet problem + cause + fix + docs-link quality.
4. High: defaults are not documented, so ergonomics cannot be judged.
5. High: copy-paste docs are too thin for adoption.
6. Medium: progressive disclosure is promising but incomplete.
7. High: escape hatches are under-specified.
8. Medium: interactive/debug elements are absent beyond a vague `--debug-context`.

##### DX Dual Voices - Consensus Table

| Dimension | Claude | Codex | Consensus |
| --- | --- | --- | --- |
| Getting started < 5 min? | No; TTHW not measurable. | No; likely internal RFC, not quickstart. | CONFIRMED gap. |
| API/CLI naming guessable? | No; names compete. | No; choose `invalidate` vs `diff`. | CONFIRMED gap. |
| Error messages actionable? | No; diagnostic contract missing. | No; examples/exit codes missing. | CONFIRMED gap. |
| Docs findable & complete? | No; copy-paste docs too thin. | No; docs IA missing. | CONFIRMED gap. |
| Upgrade path safe? | Missing. | Missing; needs experimental policy. | CONFIRMED gap. |
| Dev environment friction-free? | No; defaults and escape hatches missing. | No; demo/install not defined. | CONFIRMED gap. |

Consensus result: 6/6 confirmed DX gaps. No new User Challenge beyond invalidation-first ordering; the rest are required v1 polish for a developer-facing tool.

#### Pass 1: Getting Started

Score: 3/10 current, target 8/10.

Fix to 10: add a Quickstart with three commands or fewer:

```bash
cargo run -p tree-sitter-context -- demo invalidate --format json --explain
cargo run -p tree-sitter-context -- invalidate examples/before.rs examples/after.rs --format json --explain
cargo run -p tree-sitter-context -- benchmark smoke
```

Each command needs expected output and a fixture path. The first useful output must be invalidation, not generic chunks.

#### Pass 2: API/CLI/SDK Design

Score: 5/10 current, target 8/10.

Auto-decisions:

1. Pick one binary name for v1 examples: `ts-context`.
2. Pick one wedge command: `invalidate`; `diff` can be an alias only if documented.
3. Split API modes: `invalidate_edits` for editor edit streams and `invalidate_snapshots` for old/new source.
4. Add a Defaults table covering budget, format, language, file size, query match limit, timeout, tokenizer, parse-error behavior, and recursion.

#### Pass 3: Error Messages & Debugging

Score: 3/10 current, target 8/10.

Required diagnostic schema:

```json
{
  "code": "missing_tags_query",
  "problem": "No tags query was available for this language.",
  "cause": "The selected language configuration did not define queries/tags.scm.",
  "fix": "Run with --symbols=off or add a tags query to the language configuration.",
  "docs_url": "docs/context/diagnostics.md#missing_tags_query",
  "confidence": "degraded",
  "degraded_mode": "chunks_without_symbols",
  "exit_code": 0
}
```

Examples required for unsupported language, missing query, parse error, bad edit, budget overflow, schema mismatch, and resource limit exceeded.

#### Pass 4: Documentation & Learning

Score: 4/10 current, target 8/10.

Required docs IA:

1. Quickstart
2. CLI Reference
3. Rust API Guide
4. JSON Schema
5. Diagnostics and Exit Codes
6. Benchmarks
7. Integration Guide
8. Limitations
9. Troubleshooting

Every command must have copy-paste input and expected output.

#### Pass 5: Upgrade & Migration Path

Score: 2/10 current, target 6/10 for prototype.

The project should explicitly state an experimental contract:

- schema starts at `context.v0`,
- schema changes require snapshot updates and changelog entries,
- CLI flags can change during prototype but must warn after first external adapter,
- no stable Rust API before benchmark gate,
- migration notes required once any external user or adapter exists.

#### Pass 6: Developer Environment & Tooling

Score: 5/10 current, target 7/10.

The plan benefits from existing Rust/Cargo workflows, tree-sitter CLI patterns, and local fixtures. Gaps: CI command for context tests, non-interactive output defaults, cross-platform path normalization, workspace-root behavior, and debug/explain mode.

#### Pass 7: Community & Ecosystem

Score: 4/10 current, target 6/10.

The repository is open source and has existing tree-sitter community distribution, but this feature has no ecosystem story yet. That is acceptable for a prototype; do not build community programs now. Capture examples and benchmark reports so future adopters can evaluate without private context.

#### Pass 8: DX Measurement & Feedback Loops

Score: 5/10 current, target 8/10.

The benchmark plan is a good start, but DX measurement needs explicit TTHW checks: can a fresh checkout run the quickstart in under five minutes, and does the smoke benchmark produce a useful invalidation report? Add a `/devex-review` boomerang target after implementation.

#### DX NOT in scope

| Item | Rationale |
| --- | --- |
| Hosted playground | Too much product surface before primitive proof. |
| Published binary releases | Wait until benchmark and schema prove value. |
| Full docs website | A focused `docs/context/` section is enough for v1. |
| Community channel specific to this tool | Not needed until external adoption exists. |

#### DX What Already Exists

The repo already has CLI installation docs, command reference structure, `tree-sitter tags` and `query` docs, Rust crate README patterns, and established command naming style. The new context docs should reuse that structure instead of inventing a separate documentation style.

#### DX Scorecard

| Dimension | Score | Target | Status |
| --- | --- | --- | --- |
| Getting Started | 3/10 | 8/10 | Critical gap |
| API/CLI/SDK | 5/10 | 8/10 | Needs naming/defaults |
| Error Messages | 3/10 | 8/10 | Critical gap |
| Documentation | 4/10 | 8/10 | Needs quickstart and IA |
| Upgrade Path | 2/10 | 6/10 | Prototype policy missing |
| Dev Environment | 5/10 | 7/10 | Needs CI/debug/defaults |
| Community | 4/10 | 6/10 | Acceptable for prototype |
| DX Measurement | 5/10 | 8/10 | Needs TTHW + smoke benchmark |
| TTHW | 10-20 min estimated | 2-5 min | Not competitive |
| Competitive Rank | Needs Work | Competitive | Fails target today |
| Magical Moment | Missing | Designed via `invalidate` demo | Must add |
| Product Type | CLI Tool + Rust library | CLI Tool + Rust library | Confirmed |
| Mode | DX POLISH | DX POLISH | Confirmed |
| Overall DX | 4/10 | 7-8/10 | Must fix before implementation |

DX principle coverage:

| Principle | Status |
| --- | --- |
| Zero Friction | Gap |
| Learn by Doing | Gap |
| Fight Uncertainty | Gap |
| Opinionated + Escape Hatches | Gap |
| Code in Context | Partial |
| Magical Moments | Gap |

#### DX Implementation Checklist

- [ ] Time to hello world < 5 minutes.
- [ ] One local install/build command is documented.
- [ ] First run produces meaningful invalidation output.
- [ ] Magical moment delivered via `ts-context invalidate` or `ts-context demo invalidate`.
- [ ] Every error message has problem + cause + fix + docs link.
- [ ] API/CLI naming is guessable without docs.
- [ ] Every parameter has a sensible default and documented override.
- [ ] Docs have copy-paste examples that actually work.
- [ ] Examples show invalidation, not only chunking.
- [ ] Experimental upgrade policy exists.
- [ ] Schema version is present from day one.
- [ ] Works in CI without special configuration.
- [ ] Changelog captures schema and CLI changes.

Phase 3.5 complete. DX overall: 4/10 -> target 7-8/10. TTHW: 10-20 min estimated -> 2-5 min target. Codex: 6 concerns. Claude subagent: 8 issues. Consensus: 6/6 confirmed gaps.

<!-- AUTONOMOUS DECISION LOG -->
## Decision Audit Trail

| # | Phase | Decision | Classification | Principle | Rationale | Rejected |
|---|-------|----------|----------------|-----------|-----------|----------|
| 1 | Phase 0 | UI review skipped | Mechanical | Explicit over clever | The plan has no UI surface, screens, layout, or visual interaction flow. | Running design review on non-UI plan |
| 2 | Phase 0 | DX review required | Mechanical | Choose completeness | This is a developer-facing library/CLI used by agents and tool builders. | Treating it as pure internal engineering |
| 3 | Phase 1 | Mode set to SELECTIVE EXPANSION | Mechanical | Boil lakes | The plan is directionally right but has scope boundaries and benchmark gates that need selective additions. | Scope expansion or scope reduction |
| 4 | Phase 1 | Add benchmark smoke test by Milestone 2 | Mechanical | Boil lakes | Small in-blast-radius addition that prevents building too long without evidence. | Waiting until Milestone 5 |
| 5 | Phase 1 | Treat output schema stability as v1 scope | Mechanical | Explicit over clever | Schema is the integration contract for agent tools and adapters. | Treating serialization as bikeshedding |
| 6 | Phase 1 | Strengthen benchmark gate to workflow value | Mechanical | Choose completeness | A compression-only or micro-latency win does not prove the tool improves coding work. | Passing if any isolated dimension improves |
| 7 | Phase 1 | Keep MCP server out of v1 | Mechanical | DRY | MCP/code graph products already exist and would distract from the primitive layer. | Building a v1 MCP daemon |
| 8 | Phase 1 | Surface semantic-diff-first MVP reorder at final gate | User Challenge | Bias toward action | Both outside voices recommend changing the user's milestone order. | Auto-changing the stated milestone sequence |
| 9 | Phase 1 | Surface adapter validation track at final gate | Taste | Pragmatic | Adoption proof may require a thin adapter, but it risks pulling scope toward product work. | Forcing adapter work into v1 immediately |
| 10 | Phase 2 | Design review skipped | Mechanical | Explicit over clever | No UI scope was detected. | Running visual/design review on a library/CLI plan |
| 11 | Phase 3 | Split edit-sequence invalidation from old/new snapshot diff | Mechanical | Explicit over clever | `InputEdit` correctness is a core invariant; snapshot diff has different confidence. | One ambiguous `changed_chunks` API |
| 12 | Phase 3 | Require stable chunk identity inputs in v1 schema | Mechanical | Choose completeness | Invalidation and cache reuse require identities stronger than run-local ids. | Run-local `ChunkId` as public contract |
| 13 | Phase 3 | Keep loader discovery in CLI layer, not core by default | Taste | Pragmatic | Core reuse is higher if it accepts `Language`/`TagsConfiguration`; CLI can own loader complexity. | A stateful engine owning all discovery |
| 14 | Phase 3 | Make JSON canonical and S-expression display-only | Mechanical | Explicit over clever | Tool integration needs a stable machine contract. | S-expression as canonical v1 output |
| 15 | Phase 3 | Add resource limits and degraded partial-result diagnostics | Mechanical | Choose completeness | Arbitrary repo parsing needs bounds before real use. | Best-effort unbounded execution |
| 16 | Phase 3 | Same-file references only unless caller provides an index | Mechanical | DRY | Cross-file resolution is out of v1 and should not be implied by bundle packing. | Name-only referenced signatures presented as reliable |
| 17 | Phase 3 | Carry invalidation-first reorder to final gate | User Challenge | Bias toward action | CEO and Eng voices both recommend changing the stated milestone sequence. | Continuing generic chunking-first order without user approval |
| 18 | Phase 3.5 | Set persona to agent/tool builder or tree-sitter Rust/CLI developer | Mechanical | Bias toward action | This matches the product's developer-facing integration surface. | Generic end-user persona |
| 19 | Phase 3.5 | Target competitive TTHW of 2-5 minutes | Mechanical | Choose completeness | Developer tools must prove value quickly to earn integration time. | 10+ minute research-style setup |
| 20 | Phase 3.5 | Make `invalidate` the recommended wedge command | Taste | Explicit over clever | It names the differentiated workflow better than generic `diff`, but `diff` can remain an alias. | Leading with `chunks` or ambiguous `diff` |
| 21 | Phase 3.5 | Require concrete diagnostic schema and examples | Mechanical | Choose completeness | Problem/cause/fix/docs-link diagnostics are required for adoption and debugging. | Error registry without user-facing messages |
| 22 | Phase 3.5 | Require Quickstart, CLI Reference, Rust API, JSON Schema, Diagnostics, Benchmarks, Integration, Limitations, Troubleshooting docs IA | Mechanical | Choose completeness | Tool builders need findable docs and copy-paste examples. | Keeping all DX information buried in the RFC |
| 23 | Phase 3.5 | Add experimental schema/CLI upgrade policy | Mechanical | Explicit over clever | Prototype status is acceptable only if schema and CLI change expectations are explicit. | No upgrade story until release |

## Phase 4: Final Approval Gate

### Status

`/autoplan` review is complete through CEO, Design, Eng, and DX phases.

Decisions made: 23 total.

- Auto-decided: 18
- Taste decisions: 3
- User challenge audit rows: 2
- Unique user challenge requiring approval: 1
- **User challenge resolved: accepted invalidation-first milestone reordering**

### Auto-Decided

- CEO: keep low-level primitive scope, reject MCP/graph product scope, add earlier benchmark smoke test, strengthen benchmark gate, require stable schema.
- Design: skipped because no UI scope exists.
- Eng: split edit-sequence invalidation from snapshot diff, require stable chunk identity, make JSON canonical, add diagnostics/resource limits, keep cross-file index out of v1.
- DX: target 2-5 minute TTHW, require quickstart/docs IA, concrete diagnostics, defaults/escape hatches, and experimental schema/CLI upgrade policy.

### Taste Decisions

| Decision | Recommendation | Why surfaced |
| --- | --- | --- |
| Adapter validation track | Defer until first invalidation CLI + schema snapshots exist. | Adoption proof matters, but adapter work can pull scope toward product too early. |
| Loader discovery boundary | Keep loader discovery in CLI; core accepts `Language` / `TagsConfiguration`. | Core reusability vs convenience is a real tradeoff. |
| CLI command vocabulary | Prefer `ts-context invalidate`; optionally keep `diff` as alias. | `invalidate` names the wedge better, but `diff` is familiar. |

### User Challenge

What the user said: v1 focuses on Rust file chunking, tags integration, changed chunks, and budgeted bundles, with the original milestone order starting at chunking and putting changed chunks at Milestone 3.

What both models recommend: change the MVP order so invalidation comes first:

```text
schema + diagnostics
  -> stable chunk identity
  -> invalidate old/new snapshots and editor edit streams
  -> smoke benchmark
  -> chunks/symbols generalization
  -> bundle
```

Why: CEO, Eng, and DX voices all converged that the differentiated value is not generic AST chunking. It is one command that can explain what changed, why it believes that, what confidence it has, and what context was omitted. Chunking and symbols are still needed, but should support the invalidation proof rather than be the first visible product.

What context we might be missing: if the real goal is to contribute a conservative tree-sitter-adjacent crate with minimal strategic/product ambition, the original chunking-first sequence is easier to review and may fit maintainers better.

If the models are wrong, the cost is: invalidation-first work may force schema/identity decisions earlier and slow the simple chunking prototype.

**Approved:** invalidation-first sequence is now the v1 plan. The original chunking-first sequence is archived above for reference.

### Cross-Phase Themes

| Theme | Phases | Signal |
| --- | --- | --- |
| Invalidation-first wedge | CEO, Eng, DX | High-confidence User Challenge |
| Stable schema is product contract | CEO, Eng, DX | Auto-decided v1 requirement |
| Diagnostics and confidence must be explicit | CEO, Eng, DX | Auto-decided v1 requirement |
| Benchmark must measure workflow value | CEO, Eng, DX | Auto-decided v1 requirement |
| Avoid MCP/product scope in v1 | CEO, Eng | Auto-decided non-goal |

## GSTACK REVIEW REPORT

| Review | Trigger | Why | Runs | Status | Findings |
| --- | --- | --- | --- | --- | --- |
| CEO Review | `/plan-ceo-review` via `/autoplan` | Scope & strategy | 1 | issues_open | 7 proposals, 3 accepted, 1 unique user challenge |
| Codex Review | `/autoplan` outside voices | Independent 2nd opinion | 3 | issues_open | CEO 12 concerns, Eng 9 concerns, DX 6 concerns |
| Eng Review | `/plan-eng-review` via `/autoplan` | Architecture & tests | 1 | issues_open | 29 issues/gaps, 4 critical failure-mode gaps |
| Design Review | `/plan-design-review` | UI/UX gaps | 0 | skipped | No UI scope |
| DX Review | `/plan-devex-review` via `/autoplan` | Developer experience gaps | 1 | issues_open | score 4/10 -> target 7-8/10, TTHW 10-20 min -> 2-5 min |

- CROSS-MODEL: All phases converged on invalidation-first proof, canonical JSON schema, explicit diagnostics/confidence, and earlier workflow benchmark.
- RESOLVED: 1 unique user challenge accepted.
- VERDICT: CEO + ENG + DX review completed; invalidation-first milestone reordering approved. Implementation may proceed.
