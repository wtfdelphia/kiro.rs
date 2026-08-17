## Purpose

Provide safe, testable diagnostics for Kiro `generateAssistantResponse` EventStream traffic so protocol mapping issues can be investigated without exposing prompts, credentials, tool arguments, or reasoning signatures.

## Requirements

### Requirement: Kiro EventStream event coverage

The system MUST classify all known Kiro generation events used by the upstream response stream, including assistant text, tool use, reasoning content, context usage, and metering events. Unknown event types MUST NOT abort response processing; they MUST be counted as unknown diagnostics.

#### Scenario: Reasoning event is recognized

- **WHEN** an upstream generation stream contains a `reasoningContentEvent`
- **THEN** the system MUST classify it separately from unknown events
- **AND** response processing MUST continue

#### Scenario: Metering event is recognized

- **WHEN** an upstream generation stream contains a `meteringEvent`
- **THEN** the system MUST classify it separately from unknown events
- **AND** response processing MUST continue

#### Scenario: Unknown event is tolerated

- **WHEN** an upstream generation stream contains an unrecognized event type
- **THEN** the system MUST continue processing subsequent events
- **AND** the diagnostic summary MUST include an unknown-event count

### Requirement: Safe diagnostic summary

For each upstream generation response, the system MUST be able to produce a request-scoped diagnostic summary that contains only safe metadata. The summary MUST include event counts, recognized event type names, tool-use lifecycle counts, context usage percentage, and metering usage when available. The summary MUST NOT include raw prompts, raw tool input, raw tool output, credentials, cookies, profile ARNs, or reasoning signatures.

#### Scenario: Diagnostic summary excludes raw tool input

- **WHEN** a tool-use event contains a non-empty JSON input fragment
- **THEN** the diagnostic summary MUST include only aggregate length/count metadata for that input
- **AND** the raw input fragment MUST NOT appear in the summary

#### Scenario: Diagnostic summary excludes reasoning signature

- **WHEN** a reasoning event contains a signature
- **THEN** the diagnostic summary MUST include the signature length
- **AND** the raw signature value MUST NOT appear in the summary

#### Scenario: Diagnostic summary includes usage signals

- **WHEN** a stream contains context usage and metering events
- **THEN** the diagnostic summary MUST include the context usage percentage and metering usage metadata
- **AND** missing usage fields MUST NOT cause the request to fail

### Requirement: Tool-use lifecycle diagnostics

The system MUST aggregate tool-use diagnostics by tool-use id. For each tool-use id it MUST record the tool name, chunk count, total input length, and stop count. The system MUST detect missing stops, duplicate stops, missing ids, and missing tool names as diagnostic anomalies without leaking raw tool arguments.

#### Scenario: Multi-chunk tool call is summarized

- **WHEN** a single tool call is split across multiple tool-use events with the same id
- **THEN** the diagnostic summary MUST report one tool-use entry with the combined chunk count and total input length
- **AND** it MUST NOT emit one logical tool-use entry per chunk

#### Scenario: Tool call has one stop

- **WHEN** a tool-use id has exactly one stop event
- **THEN** the diagnostic summary MUST mark that tool-use lifecycle as complete

#### Scenario: Tool call stop anomaly is reported

- **WHEN** a tool-use id has no stop event or more than one stop event
- **THEN** the diagnostic summary MUST report a lifecycle anomaly for that id
- **AND** the raw tool input MUST remain omitted

### Requirement: Public protocol compatibility is preserved

Adding Kiro EventStream diagnostics MUST NOT change existing client-facing Anthropic or OpenAI response contracts by default. Reasoning and metering events MAY be parsed for diagnostics, but they MUST NOT be exposed in public responses unless a future spec explicitly changes that protocol behavior.

#### Scenario: Anthropic response remains compatible

- **WHEN** a client calls an existing Anthropic-compatible endpoint
- **THEN** the response shape and SSE event sequence MUST remain compatible with the pre-change contract
- **AND** diagnostic metadata MUST NOT be inserted into the public response body

#### Scenario: OpenAI response remains compatible

- **WHEN** a client calls an existing OpenAI-compatible endpoint
- **THEN** the response shape and SSE event sequence MUST remain compatible with the pre-change contract
- **AND** diagnostic metadata MUST NOT be inserted into the public response body

### Requirement: Diagnostics are verifiable without real credentials

The diagnostic behavior MUST be testable with synthetic EventStream frames or equivalent fixtures. Tests MUST NOT require real Kiro credentials, real login state, real cookies, or live upstream access.

#### Scenario: Synthetic stream verifies diagnostics

- **WHEN** a test provides a synthetic generation stream containing assistant text, multi-chunk tool use, reasoning, context usage, and metering events
- **THEN** the system MUST produce the expected diagnostic summary
- **AND** the test MUST NOT depend on a live upstream request
