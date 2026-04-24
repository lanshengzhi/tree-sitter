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

### Milestone 1: Rust file chunking

- Parse one Rust file.
- Emit top-level chunks.
- Attach names, ranges, token estimates.
- CLI: `ts-context chunks`.
- Tests using fixture Rust files.

### Milestone 2: Tags integration

- Load `tags.scm` and `locals.scm`.
- Emit definitions/references/docs.
- CLI: `ts-context symbols`.
- Tests using existing tags fixtures where possible.

### Milestone 3: Changed chunks

- Accept old/new file pair.
- Use incremental parse and `changed_ranges`.
- Map changed ranges to chunks.
- CLI: `ts-context diff`.
- Tests for body-only change, signature change, doc-only change, whitespace-only change.

### Milestone 4: Budgeted bundle

- Given file range or symbol, return packed context under budget.
- Include omitted-context metadata.
- CLI: `ts-context bundle`.

### Milestone 5: Benchmark harness

- Add repeatable benchmark tasks.
- Compare raw source and repo-map baseline.
- Produce markdown report.

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
