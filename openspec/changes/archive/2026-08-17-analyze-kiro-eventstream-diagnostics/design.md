## Context

See `proposal.md` for motivation. Current code already decodes AWS EventStream frames and maps a subset of Kiro events into Anthropic/OpenAI-compatible outputs. The latest `claude-tap` trace shows real `generateAssistantResponse` streams contain `assistantResponseEvent`, multi-chunk `toolUseEvent`, `reasoningContentEvent`, `contextUsageEvent`, and `meteringEvent`.

The main constraints are:

- Do not expose raw prompts, raw tool inputs/outputs, credentials, profile ARNs, cookies, or reasoning signatures.
- Do not change public Anthropic/OpenAI response contracts in this change.
- Keep diagnostics testable with synthetic fixtures, without live Kiro credentials.
- Preserve zero-new-warning discipline with `cargo check --release --all-targets`.

## Goals / Non-Goals

**Goals:**

- Add structured parsing for `reasoningContentEvent` and minimally structured parsing for `meteringEvent`.
- Introduce a safe per-generation diagnostic summary.
- Aggregate tool-use lifecycle metadata across multi-chunk `toolUseEvent` streams.
- Make diagnostics usable from logs/tests without changing client-facing response bodies.
- Provide tests that prove sensitive values are omitted.

**Non-Goals:**

- Do not expose `reasoningContentEvent` as Anthropic `thinking_delta` yet.
- Do not alter `message_start`, `content_block_*`, OpenAI chunk, or Responses event ordering.
- Do not add Admin UI/API surfaces in the first implementation pass.
- Do not depend on `claude-tap` schema changes.

## Decisions

### Decision 1: Add event models before response behavior

Extend Kiro event classification to recognize reasoning and metering events, but keep public response conversion unchanged by default.

Rationale:

- The trace proves these events exist, but not yet that every client expects them surfaced.
- Parsing them first lets us verify shape and frequency safely.
- It keeps the first implementation low blast-radius.

Alternatives considered:

- Map `reasoningContentEvent` directly to Anthropic `thinking_delta`. Rejected for first pass because signature handling and compatibility with non-thinking client requests need a separate protocol decision.
- Continue treating reasoning as unknown. Rejected because it hides a high-volume event family observed in real `claude-opus-5` traces.

### Decision 2: Diagnostics store lengths and counts, not values

The diagnostic summary will store event counts, tool id hashes or internal ids as needed for in-process grouping, tool names, chunk counts, input lengths, stop counts, context percentage, metering usage, reasoning text length, and signature length.

Rationale:

- Tool input and reasoning signature can contain sensitive or proprietary content.
- Length/count summaries are enough to detect mapping regressions such as missing stop events, fragmented inputs, and unrecognized event families.

Alternatives considered:

- Store redacted prefixes. Rejected because prefixes can still leak prompt/tool payload content.
- Store full JSON payload behind a debug flag. Rejected for this change because it creates an avoidable safety footgun.

### Decision 3: Keep diagnostics request-scoped and optional in logs

The first implementation should keep diagnostics as a request-scoped in-memory summary and emit only concise debug/info log lines where appropriate. It should not add persistent storage or Admin endpoints in this change.

Rationale:

- The current need is protocol analysis and regression testing, not long-term observability.
- Avoiding Admin/API changes keeps spec scope narrow.

Alternatives considered:

- Add Admin diagnostics endpoint immediately. Deferred because it would touch Admin API, UI, and security review.
- Persist summaries to disk. Rejected because it increases data-retention and redaction obligations.

### Decision 4: Reuse existing EventStream tests with synthetic frames

Tests should construct synthetic frames or use existing parser helpers to feed assistant/tool/reasoning/context/metering events through the decoder and aggregation path.

Rationale:

- Live credentials are not acceptable in CI or unit tests.
- Synthetic frames let us cover malformed and missing-field cases deterministically.

Alternatives considered:

- Use captured `claude-tap` DB fixtures. Rejected for unit tests because the DB may contain environment-specific paths and textified binary bodies that are not exact raw frames.

## Risks / Trade-offs

- Reasoning semantics remain hidden from clients -> Mitigation: document that exposing reasoning requires a later protocol-specific spec.
- Logs could become noisy on tool-heavy conversations -> Mitigation: emit compact summaries and prefer debug level unless anomalies occur.
- Tool names can reveal user tool inventory -> Mitigation: tool names are already part of public tool contracts, but avoid logging arguments/results.
- Metering payload shape may evolve -> Mitigation: parse known fields permissively and keep unknown fields out of the diagnostic summary by default.

## Migration Plan

1. Add parsing and diagnostic aggregation behind existing processing paths.
2. Add tests with synthetic streams and sensitive-value assertions.
3. Run `openspec validate --all`.
4. Run focused Rust tests for event parsing/stream handling.
5. Run `cargo check --release --all-targets` and confirm no new warnings.
6. Rollback is a code revert of the diagnostic aggregation; no schema/data migration is introduced.

## Open Questions

- Should a later change expose `reasoningContentEvent.text` as Anthropic `thinking_delta` only when the client explicitly requests thinking?
- Should metering usage eventually appear in Admin diagnostics, public usage metadata, or only logs?
- Should `claude-tap` gain Kiro-specific EventStream decoding upstream, or should `kiro-rs` provide enough local diagnostics for day-to-day analysis?
