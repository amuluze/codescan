## What changed

<!-- One or two sentences. Reference the spec/task id when applicable (e.g. .specs/tasks/llm-code-review.md). -->

## Why

<!-- The problem this solves. If it changes domain behaviour, say which invariant in
     .specs/domain/scan-engine.md is affected and whether that spec was updated first. -->

## Evidence

<!-- Which commands you ran and their result. Claims without a reproduction are not accepted. -->

- [ ] `task check` (typecheck + vitest + build + clippy -D warnings + cargo test)
- [ ] `task test:cli` (if the CLI changed)
- [ ] Rule-suite run for the affected rule prefix (if rules changed):
      `CODESCAN_FIXTURE_DIR=<prefix> CODESCAN_RULE_PREFIX=<prefix> cargo test -p codescan-engine --test rule_suite`

## Checklist

- [ ] Determinism preserved: no timestamp, absolute path, or parallel order entered a finding, scan event, or stat
- [ ] Findings still carry rule id, relative path, span, and evidence
- [ ] LLM output remains advisory (never enters findings, baselines, gate diffs, or SARIF)
- [ ] Docs updated (`docs/`, README) when behaviour or commands changed
