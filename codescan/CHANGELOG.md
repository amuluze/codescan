# Changelog

All notable changes to this project are documented in this file. The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- LLM source review on the Scan page: three-dimension coverage (security / business logic / code quality), language-round-robin file selection, paged continuation with cursor, per-batch sending with automatic split-on-truncation, per-item copy buttons
- Provider test button in Settings: minimal probe request with readable errors, never leaks the API key
- API Key moved into the Provider card for a single cohesive configuration surface
- Copy buttons for batch triage, streaming triage, and triage suggestions
- Delete scan history with two-step confirmation; findings and derived baselines cascade
- GitHub Actions CI: macOS runner, pnpm frozen install, frontend typecheck/test/build, clippy `-D warnings`, cargo test
- CodeScan logo and app icon (Tauri icon set regenerated)

### Changed

- Repository layout: CodeScan's complete Rust/pnpm workspace now lives in `codescan/`, separate from the static `website/`; desktop pages use `src/views/`, and rule fixtures use `crates/engine/tests/fixtures/rules/`
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
