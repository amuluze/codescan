# Contributing to CodeScan

Thanks for taking the time to contribute. This document covers the workflow, the local gates, and how to contribute rules.

## Quick start

```bash
git clone https://github.com/amuluze/codescan.git
cd codescan
task install
task check       # full gate — must be green before you open a pull request
task dev:app     # Tauri desktop dev mode
```

Requirements: Node ≥ 22, pnpm ≥ 10, Rust 1.96 (pinned in `codescan/rust-toolchain.toml`).

## Engineering model (spec-driven)

This repository follows a spec-driven workflow, documented in [AGENTS.md](AGENTS.md):

- `.specs/domain/scan-engine.md` — domain invariants. This is the **highest-priority constraint**; a change that violates it is a change of the domain, not a bug fix.
- `.specs/tasks/<task-id>.md` — task specs. Every acceptance criterion must be objectively verifiable (a test, a command, an observable output).
- `.plans/<task-id>.md` — implementation path. A plan must not rewrite spec acceptance criteria.

If your change alters domain behaviour (finding model, fingerprints, gating, determinism), update the domain spec **first**, then the task spec, then the code.

## Local gates

| Command | Purpose |
|---|---|
| `task check` | typecheck + vitest + build + clippy `-D warnings` + `cargo test --workspace` |
| `task test` | Frontend unit tests |
| `task test:rust` | Engine, rule suites, command layer, CLI |
| `task test:cli` | CLI black-box tests (exit codes, output formats, SARIF upload dry-run) |

Rules of thumb:

- Rust must stay clean under `cd codescan && cargo clippy --workspace --all-targets -- -D warnings`.
- TypeScript is strict; use `import type` for type-only imports; all IPC calls go through `platform.ts`.
- **Determinism is the top priority**: no timestamp, absolute path, or parallel-completion order may enter a finding, scan event, or stat.
- Prefer the narrowest test that proves the change; do not re-run whole suites for walkthroughs.

## Contributing a rule

Rules are YAML assets in `codescan/crates/engine/rules/`. A new rule needs:

1. A stable `id` (`<lang>/<slug>`) and a declared `language`, `dimension`, and `severity`.
2. A message that states **which construct was found** — never a claim of exploitability or reachability.
3. A fixture under `codescan/crates/engine/tests/fixtures/rules/` (fixture files carry `ruleid:` annotations) so the rule is unit-tested like the built-ins.
4. Rule-suite coverage: `cd codescan && CODESCAN_FIXTURE_DIR=<prefix> CODESCAN_RULE_PREFIX=<prefix> cargo test -p codescan-engine --test rule_suite`.

Semantics for AST rules, text rules, taint rules and metrics are documented in [docs/rules.md](codescan/docs/rules.md).

## Business-logic rules

Business-logic rules encode project invariants. The built-in ones are examples of the mechanism, not a generic library. If you contribute an invariant for a specific project, document the invariant it assumes.

## Pull requests

- Keep the change focused; one concern per PR.
- Describe **what changed and why**, and reference the spec/task id when applicable.
- State the evidence: which tests you ran and their result. Claims without a reproduction are not accepted.
- Update docs (`codescan/docs/`, README) when behaviour or commands change.

## Reporting bugs

Open an issue with the smallest reproduction you can: workspace shape (language, file), command or UI path, expected vs actual output, and the engine version (`engine_version` in the scan report). Please redact secrets — API keys never belong in an issue.

Security-sensitive reports should follow [SECURITY.md](SECURITY.md) instead.

## Code of Conduct

Participation is governed by [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md).

## License

Contributions are licensed under the [MIT License](LICENSE).
