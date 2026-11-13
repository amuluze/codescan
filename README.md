<p align="center"><img src="codescan/assets/brand/logo.svg" alt="CodeScan" height="76" /></p>

<p align="center">
  <img src="https://img.shields.io/badge/license-MIT-blue.svg" alt="MIT License" />
  <img src="https://img.shields.io/badge/platform-macOS%20arm64-informational" alt="macOS Apple Silicon" />
  <img src="https://img.shields.io/badge/dependencies-no%20external%20engine-success" alt="No external scanner" />
</p>

**CodeScan** is a local-first code scanner with a deterministic AST rule engine, a Tauri 2 desktop app, and a headless CLI. It covers security, business logic, and code quality with no external scanning engine and no network access during scans.

This repository hosts two independent product surfaces: the CodeScan application workspace and its static website.

## Products

| Directory | Product | Documentation |
|---|---|---|
| [`codescan/`](codescan/) | Rust engine, CLI, and Tauri desktop app | [English](codescan/README.md) · [中文](codescan/README.zh-CN.md) |
| [`website/`](website/) | Static CodeScan website; no build step or runtime dependencies | [Website guide](website/README.md) |

## Quick start

Requirements: Node ≥ 22, pnpm ≥ 10, and Rust 1.96 (pinned in [`codescan/rust-toolchain.toml`](codescan/rust-toolchain.toml)). Desktop packaging is supported only on macOS Apple Silicon (arm64).

Run tasks from the repository root:

```bash
task install   # install CodeScan dependencies
task dev:app   # Tauri desktop development mode
task check     # typecheck + Vitest + frontend build + Clippy + Rust tests + notices generation
```

Other common entry points:

| Task | Purpose |
|---|---|
| `task dev` | Browser-only development mode without native capabilities |
| `task test:cli` | CLI black-box tests |
| `task scan` | Scan the CodeScan workspace with its own engine |
| `task build:dmg` | Build the macOS arm64 `.app` and `.dmg` |
| `task --list` | List all available tasks |

The website needs no package manager or build step:

```bash
cd website
python3 -m http.server 8000
# open http://localhost:8000
```

## Repository layout

```text
.
├── codescan/             # CodeScan application workspace
├── website/              # independent static website
├── .specs/               # domain invariants and task specs
├── .plans/               # implementation plans
├── .github/              # CI and repository automation
├── AGENTS.md             # cross-product engineering conventions
├── Taskfile.yml          # task orchestration
├── package.json          # root command forwarding
└── pnpm-workspace.yaml   # root-only pnpm config; no application dependencies
```

Command implementations live in [`codescan/package.json`](codescan/package.json), and the CodeScan workspace configuration lives in [`codescan/pnpm-workspace.yaml`](codescan/pnpm-workspace.yaml). The website does not share the CodeScan Node or Rust workspace.

## Repository governance

- [Contributing](CONTRIBUTING.md)
- [Engineering conventions](AGENTS.md) · [CodeScan](codescan/AGENTS.md) · [website](website/AGENTS.md)
- [Domain invariants](.specs/domain/scan-engine.md)
- [Task specifications](.specs/tasks/)
- [Security policy](SECURITY.md)
- [Code of Conduct](CODE_OF_CONDUCT.md)

## License

[MIT](LICENSE)
