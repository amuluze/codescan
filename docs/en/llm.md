# LLM assist layer (English)

The provider model mirrors the axiom project: OpenAI-compatible and Anthropic-compatible, BYO API key, no built-in telemetry.

The [Chinese version of this document](../llm.md) is the original; this file mirrors it.

## Configuration

In Settings, fill in provider kind, base URL, model, and API key:

- The API key is written only to the `secrets` table in `~/.codescan/codescan.db` (Rust-exclusive read/write); the command layer exposes only "configured / not configured" and has no plaintext read-back path.
- Requests go only to the base URL you configure: OpenAI uses `{base}/chat/completions` (Bearer), Anthropic uses `{base}/v1/messages` (`x-api-key` + `anthropic-version`).
- Timeouts: 60s non-streaming, 120s streaming; error messages never contain the key.

## Capabilities

### 1. Per-finding triage (reference)

Click "LLM 分诊" in the finding detail; the model returns `true-positive | likely-false-positive | uncertain` with a rationale. Display only — **not written to findings, not part of baseline/gate**.

### 2. Streaming triage

Click "流式分诊"; the Rust side parses SSE incrementally and emits `llm://delta` events:

```json
{ "requestId": "<this request id>", "text": "delta text" }
```

The UI renders only deltas matching the current `requestId`; the command returns the full text at the end. The parser handles OpenAI `choices[0].delta.content` and Anthropic `content_block_delta.delta.text`, tolerates `[DONE]`, empty deltas, non-JSON lines, and chunk boundaries splitting a UTF-8 codepoint.

### 3. Batch triage

Click "批量分诊"; the command `llm_batch_triage(scan_id, fingerprints)` triages up to 20 findings at once. The UI sends the first 20 fingerprints of the current filtered list; an empty array falls back to a deterministic order (severity, path, position, rule id). Each entry carries `fingerprint`, `assessment`, `rationale`; missing or extra fingerprints are marked `missing: true`.

### 4. Candidate rule write-back

Click "生成候选规则"; the model produces a `custom/`-prefixed rule YAML. Rust validates it (query syntax, enums, id uniqueness, no collision with built-ins). Only after you explicitly click "保存为自定义规则" is it written to `~/.codescan/rules/custom.yaml` and effective in the next scan.

### 5. Isolated candidate regression

"运行回归验证" runs `llm_candidate_regression(scan_id, fingerprint, candidate_yaml, save_fixture)` against a **temporary corpus** — no LLM call, no network. It copies the seed file, inserts a `// ruleid:` / `# ruleid:` annotation above the finding line, scans the temp directory with only the candidate rule, and reports missed / over-matched lines. The result is display-only and never changes findings, baselines, or gate results.

## Discipline and boundaries

- LLM output never enters the gate; only saved deterministic rules affect scans and baselines.
- Triage (single, streaming, batch) is display-layer reference material — it never writes findings, baselines, history, or gate diffs.
- Failures are recoverable: missing key, network error, or unparsable model output all return a readable error and leave scan/history/baseline untouched.
