# CodeScan repository

This repository contains two separate product surfaces:

| Directory | Product | Documentation |
|---|---|---|
| [`codescan/`](codescan/) | Local-first deterministic code scanner: Rust engine, CLI, and Tauri desktop app | [English](codescan/README.md) · [中文](codescan/README.zh-CN.md) |
| [`website/`](website/) | Independent static CodeScan website; no build step or runtime dependencies | [Website guide](website/README.md) |

## Development

Run tasks from the repository root. The root `package.json` and root-only `pnpm-workspace.yaml` only orchestrate commands; command implementations live in `codescan/package.json`, and the CodeScan workspace config lives in `codescan/pnpm-workspace.yaml`.

```bash
task install
task dev:app
task check
```

Requirements: Node ≥ 22, pnpm ≥ 10, Rust 1.96, and macOS Apple Silicon for desktop packaging. The Rust toolchain is pinned in [`codescan/rust-toolchain.toml`](codescan/rust-toolchain.toml).

## Repository governance

- [Contributing](CONTRIBUTING.md)
- [Engineering conventions](AGENTS.md) · [CodeScan](codescan/AGENTS.md) · [website](website/AGENTS.md)
- [Domain invariants](.specs/domain/scan-engine.md)
- [Task specifications](.specs/tasks/)
- [Security policy](SECURITY.md)
- [Code of Conduct](CODE_OF_CONDUCT.md)

The website remains a plain HTML/CSS/JavaScript site. It does not share the CodeScan Node or Rust workspace.
