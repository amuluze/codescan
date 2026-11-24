# LLM assist layer (English)

The provider model mirrors the axiom project: OpenAI-compatible and Anthropic-compatible, BYO API key, no built-in telemetry.

The [Chinese version of this document](../llm.md) is the original; this file mirrors it.

## Configuration

Settings stores **multiple provider profiles** (name / kind / base URL / model), each with its own API key; exactly one "default" profile is the fallback for all LLM calls, and the legacy single configuration migrates on first read (Domain 3a.2):

- API keys are stored per profile in the `secrets` table of `~/.codescan/codescan.db` (key `llm:api_key:<id>`, Rust-exclusive read/write); the command layer exposes only "configured / not configured" and has no plaintext read-back path.
- Requests go only to the selected profile's base URL: OpenAI uses `{base}/chat/completions` (Bearer), Anthropic uses `{base}/v1/messages` (`x-api-key` + `anthropic-version`).
- Source review can pick a different profile explicitly via the "模型" dropdown on the Scan page; triage and other calls always use the default profile.
- Timeouts: 60s non-streaming, 120s streaming; error messages never contain the key.
- Transport errors show the full underlying cause chain (DNS / TCP / TLS / proxy / timeout) plus a short troubleshooting hint; requests honor `HTTPS_PROXY` / `ALL_PROXY` environment variables.

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

### 6. Workspace source review (reference)

Click "开始审查" on the Scan page; the command `llm_review_workspace(root, max_files, offset, provider_id)` sends candidate source files to the selected provider profile in batches (at most 24 files, ≤128 KiB per run) and pages automatically by cursor until all candidates are covered; "重置进度" aborts and clears. The "模型" dropdown in the review card switches profiles; the default is the active profile from Settings.

- Each suggestion carries `path / line / severity / category / title / detail / confidence`; duplicates across pages are deduplicated by (path, line, title); unparsable or over-budget entries are counted as `dropped` and shown in the coverage summary.
- The review prompt embeds a versioned built-in skill pack (`apps/desktop/src-tauri/skills/llm-review/`, version `llm-review/v1`): review protocol, false-positive gates (config values, framework mitigations, test/dead code, unreachable paths), severity/confidence calibration, and separate security / business-logic / quality checklists. The pack is compiled into the binary with `include_str!`; nothing is fetched or executed at runtime.
- Settings → "LLM 审查内置 Skill" lets you inspect and override each skill file. Overrides live in `~/.codescan/skills/llm-review.json` (≤16 KiB per file, ≤32 KiB total); content equal to the built-in body counts as not overridden. Corrupt or invalid entries fall back to the built-in text at load time. Editing does not change the untrusted-source declaration, JSON output validation, verification verdicts, or gate boundaries.
- Independent verification is on by default and can be disabled in the review card: after each hunt batch, the source fragments and candidate list go through a second, independent verdict pass (`confirmed / needs_validation / rejected`). `rejected` items are dropped and counted (summary: "复核过滤 N 条"); `needs_validation` items stay visible with a "待验证" badge. Missing, invalid, or failed verification degrades that batch to `needs_validation` without failing the review. With verification off, no second request is sent and suggestions carry no verification field.
- Single-item copy and Markdown/JSON exports include the verification state; across pages, `confirmed` wins over `needs_validation` for the same suggestion. Verification is still a model judgment, not proof of exploitability.
- Since v1.5.0 the review card shows a result stats grid: suggestion total plus error / warning / info counts, reusing the deterministic scan result grid and tinted by severity (left bar and label). The coverage summary (files reviewed, oversized-file exclusions, dropped counts) sits on its own line below the control row so it no longer competes with the dropdowns for width. Display-only — never written to findings, baselines, or gates.
- Large files: files over 64 KiB are never sent; the count surfaces in the summary as "另有 N 个超大文件（>64 KiB）未纳入审查" so the coverage blind spot is visible. Minified/generated shapes (fewer than 4 lines and over 16 KiB) are silently skipped at read time (cursor consumed, same as binary files), preventing the line-split cascade from aborting the whole page.
- Results can be filtered by severity / dimension / confidence / keyword, and exported as Markdown + JSON.

## Discipline and boundaries

- LLM output never enters the gate; only saved deterministic rules affect scans and baselines.
- Triage (single, streaming, batch) is display-layer reference material — it never writes findings, baselines, history, or gate diffs.
- Failures are recoverable: missing key, network error, or unparsable model output all return a readable error and leave scan/history/baseline untouched.
