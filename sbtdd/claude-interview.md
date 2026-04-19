# Interview Transcript: magi-core Implementation

## Q1: Feature flags for providers

**Q:** How should feature flags work for providers? The spec mentions `feature = "claude"`, `feature = "gemini"`, `feature = "openai"`. Should CLI providers (ClaudeCliProvider) be behind a separate feature flag, or bundled with their HTTP counterpart?

**A:** Separate flags. E.g., `claude-api` and `claude-cli` as independent features.

---

## Q2: Logging/tracing strategy

**Q:** What logging/tracing strategy should magi-core use? The crate needs to report agent progress (init banner, per-agent status, consensus).

**A:** `tracing` crate. Structured logging with spans; standard in async Rust ecosystem.

---

## Q3: Testing LLM interactions / mocks

**Q:** For testing LLM interactions, how should mocks work? The spec uses `Arc<dyn LlmProvider>` which is easily mockable.

**A:** `mockall` crate. Auto-generate mocks from trait definitions.

---

## Q4: Retry logic for failed LLM calls

**Q:** Should magi-core include retry logic for failed LLM calls (e.g., transient HTTP 500, rate limits), or should the caller handle retries?

**A:** Built-in retry with backoff. Configurable via `RetryConfig` in `MagiConfig`. Default: 3 retries, 1s base delay between each. Only transient errors (500, 429, timeouts) are retryable.

---

## Q5: System prompt storage and override mechanism

**Q:** The spec embeds system prompts as `const &str` in code. Should there also be a runtime override mechanism beyond the existing `with_custom_prompt` and `from_directory`?

**A:** Default prompts stored in separate `.md` files, compiled into the binary with `include_str!`. This way, maintaining or improving a default prompt only requires modifying the `.md` file, not the source code. Runtime overrides via `with_custom_prompt()` and `from_directory()` remain as specified.

---

## Q6: Prelude module

**Q:** Para la API publica del crate, prefieres un modulo prelude para imports ergonomicos o imports explicitos desde cada modulo?

**A:** Prelude module. `use magi_core::prelude::*` imports Magi, Mode, MagiReport, LlmProvider, etc.

---

## Q7: Retry configuration defaults

**Q:** Para el retry con backoff, cual deberia ser la politica default?

**A:** Configurable via `RetryConfig`. Default: 3 retries, 1s separation between each retry. User can customize via `MagiConfig`.

---

## Q8: Deployment / integration model

**Q:** Hay alguna consideracion de deployment o integracion que deba tener en cuenta?

**A:** Lib + ejemplo CLI. The crate is a library as the primary product, with a binary example in `examples/` demonstrating usage.

---

## Q9: Platform portability (WASM, no tokio::process)

**Q:** Para WASM o plataformas sin tokio::process, deberia el crate compilar sin los CLI providers?

**A:** Fuera de scope. Only platforms with full tokio support (Linux, macOS, Windows).

---

## Q10: MSRV (Minimum Supported Rust Version)

**Q:** Algun requisito de MSRV o solo latest stable?

**A:** Latest stable (1.85+). Edition 2024, no concern for older versions.

---

## Q11: API publica - ergonomia

**Q:** Que consideraciones de ergonomia general tienes para la API publica?

**A:** Estilo reqwest/tokio. Follow conventions of popular Rust ecosystem crates: builder pattern with method chaining, sensible defaults, `Result` returns, strongly typed.
