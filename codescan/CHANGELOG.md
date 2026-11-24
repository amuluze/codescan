# Changelog

All notable changes to this project are documented in this file. The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).


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
