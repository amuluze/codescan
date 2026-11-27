# Changelog

All notable changes to this project are documented in this file. The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).



## [1.6.2] - 2026-11-27

### Changed

- Rule page: the rule table, pack management and read-only rule detail now live in `views/RulesView.parts.tsx` as view-private blocks (the editor stays a `components/` dialog); `useRulePacks` owns pack state and actions.

### Fixed

- Self-scan remediation: file reads carry explicit upper bounds (`codescan_engine::fsutil`); no `unwrap`/`expect`/`unreachable!` on reachable paths in the desktop command layer, scan manager, store, LLM retry loop, parser and CLI. Lock poisoning is recovered instead of panicking, a panicking scan thread releases the "scan in progress" registration, schema migrations surface real errors, and legacy Provider profile/key migration plus profile deletion run in a single transaction (a failed key copy no longer deletes the old key).
- `export_sarif` takes a directory and derives `codescan-<scan_id>.sarif.json` in Rust, matching `export_report`'s path boundary.
- Metric clean-up: split the guarded/flow/constant walkers, per-language import extraction, rule-set validation, and the remaining large views into smaller units without changing scan output (`rule_suite` and integration fixtures unchanged).
- `.codescanignore` now skips `crates/engine/tests/fixtures/`: rule fixtures are deliberate hits verified by `rule_suite`, not repository findings.

## [1.6.1] - 2026-11-27

### Added

- Custom-rule creation now opens a rule editor dialog and appends a single rule through `save_custom_rule`, preserving existing custom rules; editing an existing custom rule still saves the full `custom.yaml`.
- Text rules support optional `text_exclude` regexes; `text/http-url` ignores XML/SVG namespaces, Apple plist DTDs, MPL license URLs, and local IPC URLs.

### Fixed

- `metric/unreachable-code` now ignores `line_comment`/`block_comment` nodes and does not treat `panic!`/`unreachable!()` nested inside loops or match arms as terminating the enclosing statement.
- `rust/fs-read-unbounded` now skips reads in functions that already gate on `metadata.len()`, and Rust heuristic rules skip `mod tests` code.
- Pattern/text rules support `exclude_paths`; Rust heuristics skip top-level `tests/*.rs` and `text/todo-debt` skips docs/specs/rule definitions.
- `metric/duplicate-block` supports `skip_tests`; test files and Rust `mod tests` no longer dominate duplicate-code findings.
- All file-level metrics support `skip_tests`; built-in complexity/length/nesting/unreachable metrics skip test files and Rust `mod tests` by default.
- Split the rules table and rule-pack management out of `RulesView`; the view function is no longer flagged as overlong.

## [1.6.0] - 2026-11-27

### Added

- Taint analysis now drops unreachable local sinks/assignments, carries `&&` condition guards across calls, and replays switch-case effects with case-value guards; Java field receivers use a dedicated `java/read-object-field` rule bound to `ObjectInputStream` fields, and child_process member calls ignore same-named local module objects.
- Guarded cross-function field-write/return-alias summaries; child_process member calls now require the module receiver; Flask `request as req` sources are alias-bound.

- Cross-function taint propagation now carries parameter field-write effects (`callee(o, v) { o.field = v }`) and parameter object identity sets (`id(o) { return o }`, `choose(a,b) { return flag ? a : b }`) back to caller state, pruning conditional branches when the condition is statically known or assigned from a local constant at the expression site, merging if/else, loop, switch (including fallthrough), and try/catch/finally branches, with break/continue and labeled break/continue-aware flow, replaying matching continue states through the next loop iteration; labels remain function-local, and following statically known if conditions, including local aliases and nested fields.

- Go taint sanitizers now recognize aliased and dot imports for `strconv.Atoi` and `filepath.Clean`; Flask `request` aliases (`req.args`, `flask.request.args`) are recognized as taint sources.

- Taint aliases for Go package aliases/dot imports (`exec.Command`, `os.Open`, `net/http`) and Rust `use ... as`/glob imports (`Command::new`, `fs::write`).

- Go dot-import rules for `exec.Command`, `md5`/`sha1`, and `math/rand`; Rust glob-import rules for `std::fs::read*`, `std::mem::transmute`/`forget`, and `std::process::Command::new`.

- Rule library batch 2: Java XMLDecoder and URL.openConnection heuristics, Rust unsafe-block visibility, and Flask debug=True detection, with fixtures and quality-gate coverage
- Local rule packs: `pack.yaml` manifests, installed under the app data root with content-hash integrity checks, merged into scans and shown as read-only rules with `id@version` origin; scan history records pack id/version/integrity; CLI `pack install|list|remove` shares the same engine implementation and `scan` loads installed packs; optional Ed25519 pack signatures with trusted public keys (`pack keygen|sign|trust`)
- Rule library batch 1: prototype pollution and string child-process execution for TypeScript, plus `requests verify=False` and Django `mark_safe` for Python; documented the rule quality gates
- Rule page now edits custom rules in-app: full YAML editor, compile validation, atomic save, and a smoke test that runs only the custom rules against a sample file
- v2 type layer for Java: inheritance rules resolve `extends`/`implements` closures across files, interfaces, wildcard imports, and anonymous class bodies; `java/trust-all-certs-inherited` covers indirect trust managers without relying on empty-method heuristics
- v2 dataflow for Go: `go/plaintext-http` is now a taint rule; `http://` literals are proven to reach `http.Get/Head/Post/PostForm` through variables, parameters, concatenation, function-local `const`/`var`, and same-file package-level initialization, while `https://` and unproven variables are not reported

### Fixed

- Taint call/effect flow now skips unreachable branches, applies loop/switch guards with case-value matching; child_process bare/member calls skip local shadowing; readObject field receivers are type-bound; Flask module aliases (`flask.request.args`) resolve as sources.

- `java/read-object-field` recognizes classes that hold an `ObjectInputStream` field, covering field-based cross-method reads in addition to `java/read-object` parameters and locals.

- Go `tls-insecure-skip-verify` now resolves `crypto/tls` import aliases; `java/read-object` only reports inside methods that expose an `ObjectInputStream` parameter or local.

- `postMessage` wildcard rules now inspect the target origin (second argument) instead of any string argument; `py/taint-os-system-request` only sources `request.args`; `go/tls-insecure-skip-verify` requires an enclosing `tls.Config`. Negative fixtures cover safe-origin, unrelated `.args`, and custom-struct cases.
- `py/yaml-load-without-loader` accepts positional `SafeLoader`/`CSafeLoader`; `java/read-object` requires an explicit receiver and is warning-level; `go/unsafe-template` checks the `text/template` import so `html/template` is no longer reported. Negative fixtures cover all three.
- Pattern rules support an optional `imports` regex gate and `imported_symbols` alias mapping; `ts/child-process-string`, `py/weak-hash`, and `py/os-system` cover aliased, destructured, and renamed imports. Go package aliases (`os/exec`, `crypto/md5`, `math/rand`) and Rust `use ... as` aliases (`Command`, `fs`, `mem`), including nested use trees, are resolved too. Also cover fully qualified `java.security.MessageDigest`, `window.eval`, and `std::fs::write` taint sinks.
- Metric findings now use each rule's declared id/dimension/severity/message; disabling `metric/duplicate-block` no longer emits default duplicate-block findings, and custom metric rules apply their own thresholds and metadata
- Pattern, filter, and taint queries without `@match` are rejected at compile time instead of silently producing no findings; multiple `@match` captures choose the smallest span
- Non-UTF-8 source files are parsed from the same lossy text used for matching, preventing misplaced AST offsets and silent misses
- Disabled custom rules with YAML schema/field errors no longer block the rest of the rule set; errors are retained on the engine and surfaced as rule rows in the rules page
- Duplicate-block windows are keyed by SHA-256 content hashes instead of a 64-bit quick hash, eliminating collision-driven false duplicate groups
- Scan history now persists scan events and the enabled rule set/custom-rule hash, exposes them through `get_scan_events`, and SARIF export uses the real engine version instead of `"persisted"`

### Changed

- TSX and JSX now have their own built-in rule declarations (15 new rules), so TypeScript/JavaScript AST and taint rules are no longer silently absent on React files
- Text rules scan hidden files and unknown extensions (`.env`, YAML, JSON, etc.); `.git` remains excluded, and `files_scanned` counts every processed text file
- Built-in precision: `py/yaml-load-without-loader` only accepts `SafeLoader`/`CSafeLoader`, `go/ignored-error` requires a call and avoids duplicate/`_ = literal` hits, `java/trust-all-certs` requires a direct/qualified/anonymous `X509TrustManager` implementation, and a new `rust/sql-format-interpolation` rule covers named `format!` interpolation without changing `rust/sql-string-concat` semantics

## [1.5.8] - 2026-11-25

### Fixed

- LLM source review now treats a truncated response body (`finish_reason: length` / `stop_reason: max_tokens` with partial text, or an unterminated JSON array) as output-budget truncation and retries with a smaller batch or line range; previously it surfaced as `响应不是 JSON 数组` with the partial JSON and aborted the page

## [1.5.7] - 2026-11-25

### Added

- LLM source review now retries 429 / 5xx / 529 responses up to five times with exponential backoff, honoring `Retry-After` (30s cap); the policy applies to both hunt and verification requests
- When retries are exhausted or another unrecoverable error occurs, completed batches from the failed page are kept and returned with a readable error; the cursor advances only past fully reviewed files, so "继续审查" resumes the remaining files without redoing completed ones

## [1.5.6] - 2026-11-25

### Changed

- Provider profile add/edit now uses a dedicated dialog that submits the profile fields and API key together: new profiles can set a key immediately, editing keeps the existing key when the field is left empty, and an explicit checkbox clears it; the separate key commands and page-level key block were removed
- The provider dialog keeps the existing validation, effective-URL preview, discard confirmation, row highlight, and busy states; the underlying `upsert_provider_profile` command now accepts optional `apiKey`/`clearKey` and persists them in one save

## [1.5.5] - 2026-11-25

### Fixed

- Provider profile editor: key drafts no longer leak across profiles, unsaved field/key changes require explicit discard confirmation, and save is blocked until name / base URL (http(s)) / model pass the same validation as the backend
- Provider profile editor now previews the effective request URL (OpenAI `/chat/completions`, Anthropic `/v1/messages`) and flags duplicate path suffixes; the edited row is highlighted, existing-profile saves close the editor, new-profile saves stay open for key setup, test/delete/default-switch results name the profile, and in-flight actions cannot be double-submitted

## [1.5.4] - 2026-11-24

### Added

- LLM source review now embeds a versioned built-in review skill pack (protocol, false-positive gates, severity/confidence calibration, and per-dimension checklists) instead of a fixed prompt
- Optional independent verification (on by default) re-checks each batch's suggestions against the cited source, drops explicitly rejected candidates, marks unconfirmed ones as "needs validation", and degrades gracefully when verification fails; the review card exposes a toggle, per-item badges, and verification counts, and exports carry the verification state
- Settings now lists the built-in review skill files with per-file view/edit, override status, reset-to-default, byte limits, and readable save errors; overrides are stored under the local data root and are used by the next review

## [1.5.3] - 2026-11-24

### Changed

- Scan page review card layout: controls, coverage summary, severity stats, filters, and the suggestion list now use consistent spacing; the coverage summary moves to its own line instead of competing with the model dropdown for width; stat cards are tinted by severity; long suggestion paths truncate with a full-path tooltip

## [1.5.2] - 2026-11-24

### Added

- Multiple provider profiles: Settings stores any number of profiles (name / kind / base URL / model) with per-profile API keys and one active default; the legacy single configuration migrates automatically on first read
- The Scan page review card gains a model dropdown to run the review with any configured profile; triage and other LLM calls use the active profile

## [1.5.1] - 2026-11-24

### Changed

- Export completion dialog copy lists each written file on its own line, so long base names no longer wrap mid-name
- LLM transport errors (triage, review, streaming) now include the full underlying cause chain (DNS / TCP / TLS / proxy / timeout) plus a short troubleshooting hint, instead of the bare reqwest top-level message
- LLM source review now reports how many oversized source files (>64 KiB) were excluded, shown in the review summary so full-candidate coverage can no longer hide the blind spot; minified/generated files (fewer than 4 lines, over 16 KiB) are pre-filtered at read time instead of triggering the page-aborting line-split failure

## [1.5.0] - 2026-11-24

### Added

- LLM source review results on the Scan page now show a stats grid (suggestion total plus error / warning / info counts), mirroring the deterministic scan result stats; the inline summary keeps only coverage (files reviewed) and dropped counts

## [1.4.0] - 2026-11-20

### Added

- The app now checks for updates on startup and every 6 hours; when a newer release is found, an upgrade icon appears before the version number in the header and clicking it opens the Settings page to download and install the update
- The Scan page offers a quick-select of recently scanned workspaces beside the workspace picker: entries are deduplicated, ordered most recent first, capped at 8, and refresh after each scan

## [1.3.1] - 2026-11-19

### Fixed

- LLM source review no longer fails outright when a single large file exhausts the thinking budget: the file is split by line ranges and retried, suggestions keep absolute file line numbers, and duplicate suggestions across fragments are deduplicated

## [1.3.0] - 2026-11-19

### Changed

- Exports from the review card and the findings toolbar show an in-progress state on the button and report completion or failure in a dialog, including the written file names and directory

## [1.2.0] - 2026-11-19

### Added

- App header shows the running build version, injected from `tauri.conf.json` at build time
- LLM source review results are filterable by severity, dimension, confidence, and keyword, and can be exported as Markdown and JSON to a chosen directory
- Findings page exports the currently filtered findings as Markdown and JSON (`codescan-findings-<scanId>`), alongside the existing SARIF export

### Changed

- LLM review card height now follows its content (page scrolls naturally); narrow screens cap the card and scroll inside it
- Thinking-model truncation errors show a short thinking excerpt instead of dumping the raw provider JSON
- Releases are now published only on the public repository: the Release workflow is guarded to `amuluze/codescan`, and the private repo no longer creates version tags or GitHub Releases

## [1.1.0] - 2026-11-19

### Added

- GitHub Actions CI on pushes to `main` and pull requests, including third-party notice generation checks

### Changed

- Third-party license notices are generated during check, development, and release builds and bundled into the desktop app; the generated file is no longer tracked
- Repository layout: CodeScan's complete Rust/pnpm workspace lives in `codescan/`; desktop pages use `src/views/`, and rule fixtures use `crates/engine/tests/fixtures/rules/`. Official website source is maintained outside this repository.
- Updated Rust and JavaScript dependencies, including tree-sitter 0.27 and rusqlite 0.41
- Updated `sha2` to 0.11.1 with explicit lowercase-hex encoding; existing content hashes and fingerprints remain byte-identical
- LLM source review now pages automatically: after the first run it keeps requesting the next cursor page until all candidates are covered; failures keep reviewed results and resume from the cursor, and Reset aborts the run

## [1.0.0] - 2026-11-12

### Added

- LLM source review on the Scan page: three-dimension coverage (security / business logic / code quality), language-round-robin file selection, paged continuation with cursor, per-batch sending with automatic split-on-truncation, per-item copy buttons
- Provider test button in Settings: minimal probe request with readable errors, never leaks the API key
- API Key moved into the Provider card for a single cohesive configuration surface
- Copy buttons for batch triage, streaming triage, and triage suggestions
- Delete scan history with two-step confirmation; findings and derived baselines cascade
- CodeScan logo and app icon (Tauri icon set regenerated)

### Changed

- Anthropic response parsing: skip thinking blocks, concatenate text blocks (fixes `content[0].text` failure on extended-thinking responses)
- Review requests batch at 4 files / 32 KiB per request with automatic split on `max_tokens` truncation
- Scan / Findings / Settings pages: unified input and select height (36px), consistent card layout, responsive detail pane
- Directory picker follows directory symlinks (`/tmp` and similar on macOS)
- Vite dev server sends `Cache-Control: no-store` to prevent stale WebView module cache
- `pnpm-workspace.yaml`: `allowBuilds.esbuild = true` (fixes `ERR_PNPM_IGNORED_BUILDS` in CI)

### Fixed

- Anthropic thinking-block response parsing
- `list_directory` skipping symlinked directories
- Findings page empty-state layout in narrow panes

## [0.1.0]

Initial release.

### Added

- Deterministic AST rule engine (tree-sitter): 73 built-in rules across TypeScript/TSX, JavaScript/JSX, Python, Java, Go, Rust, plus language-agnostic metrics and text rules
- Three-dimension finding model: `security`, `business-logic`, `quality`
- Quality metrics: cyclomatic complexity, function length, nesting depth, duplicate blocks, unreachable code
- Taint analysis: intra-procedural approximation plus explicit-import cross-file propagation with points-to object sets and field sensitivity
- Stable fingerprints and total ordering; SARIF 2.1.0 export
- Baseline and gate diff by fingerprint equality
- Tauri 2 + React 19 desktop app: Scan, Findings, Rules, History, Settings
- `codescan` CLI: `scan` (text / json / sarif, baseline, gate) and `github-upload` (GitHub Code Scanning)
- LLM assist layer: per-finding triage, streaming triage, batch triage, candidate rule generation with isolated regression
- Ignore sources: `.gitignore`, `.codescanignore`, global `~/.codescan/ignore`, built-in default directories
- Local data root `~/.codescan/` with Rust-owned SQLite secrets table
