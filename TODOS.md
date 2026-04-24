# TODOs

## Deferred from `/autoplan` Review

### Adapter validation track

- What: Add one thin real-agent adapter or integration spike after the first useful invalidation CLI exists.
- Why: CEO and Eng reviews both found that adoption proof is the main risk; a clean Rust primitive is not enough if no agent workflow uses it.
- Pros: Validates schema usability, output size, and integration friction before hardening the API.
- Cons: Pulls scope toward product work and may distract from primitive correctness if started too early.
- Context: Do this only after canonical JSON snapshots and `diff` / `invalidate` smoke benchmarks exist. Candidate adapters include a local CLI wrapper for an existing agent workflow or a minimal MCP proof that consumes the JSON output without becoming the product.
- Depends on / blocked by: Stable JSON schema, invalidation CLI, smoke benchmark.

### Multi-language expansion

- What: Add Python and TypeScript context fixtures after Rust v1 passes benchmark gates.
- Why: Agent adoption happens in mixed repositories; Rust-only success may not generalize.
- Pros: Catches grammar/query variance and schema assumptions early.
- Cons: Adds fixture and query complexity before the Rust proof is complete.
- Context: Keep Rust as the first benchmark language. Expand only after chunk identity, diagnostics, and invalidation semantics are stable enough to compare across languages.
- Depends on / blocked by: Rust v1 benchmark passing.

### Release and distribution pipeline

- What: Define how `tree-sitter-context` is built, packaged, and installed if it graduates from prototype.
- Why: A CLI/library without distribution cannot be adopted outside local development.
- Pros: Makes DX real and gives users a predictable install path.
- Cons: Premature release work can create maintenance burden before value is proven.
- Context: Current v1 validation can use local `cargo` workflows. Publishing, binary releases, or CLI integration should wait until benchmark and adapter evidence are positive.
- Depends on / blocked by: Final placement decision and benchmark proof.

### Hosted playground

- What: Consider a zero-install playground or recorded demo for `tree-sitter-context`.
- Why: DX review found that the magical moment currently requires local setup; a playground could reduce evaluation friction later.
- Pros: Faster evaluation and easier sharing of benchmark/invalidation examples.
- Cons: Product surface, hosting, and maintenance are too much before primitive proof.
- Context: Do not build this in v1. Revisit only after invalidation demo, JSON schema, and smoke benchmark are stable.
- Depends on / blocked by: Stable invalidation output and benchmark proof.

### Context-specific community channel

- What: Decide whether `tree-sitter-context` needs a dedicated discussion/support channel.
- Why: If external adapters appear, integration questions may not fit existing tree-sitter parser-development channels.
- Pros: Gives adopters a place to ask schema, benchmark, and adapter questions.
- Cons: Premature community surface creates support burden before adoption exists.
- Context: Not needed during prototype. Revisit after at least one external adapter or serious user.
- Depends on / blocked by: External adoption signal.
