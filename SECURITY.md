# Security Policy

## Scope

CodeScan is a **local** tool: it scans a workspace on your machine, stores history in `~/.codescan/`, and only talks to a network when you explicitly configure an LLM provider or upload SARIF yourself. Most of the attack surface is therefore local.

This policy covers the repository, the released desktop app, the CLI, and the published rule assets.

## Reporting a vulnerability

Please do **not** open a public issue for a security problem.

Report it privately via **GitHub Security Advisories**: *Security* → *Report a vulnerability* on the repository page. If that is unavailable to you, open a regular issue with the title `[security]` and no technical details, so a private channel can be arranged.

Please include:

- affected version (app version / `engine_version` in the scan report / commit SHA),
- reproduction steps or a minimal workspace that triggers the issue,
- the impact you observed (crash, incorrect scan result, secret exposure, file write outside the data root, …),
- your platform and architecture.

Do not include API keys or other credentials in the report; redact them.

## Response

- Acknowledgement: within 3 working days.
- Triage and severity assessment: within 10 working days.
- Fix or mitigation plan: communicated as soon as severity is known.

We aim to release a fix for confirmed issues in the next patch release, and to credit reporters in the release notes unless you prefer otherwise.

## Out of scope (by design)

- **LLM output is advisory.** Suggestions produced by a configured LLM provider are non-deterministic reference material and never gate a build. A wrong LLM answer is a product limitation, not a vulnerability — report it as a bug if the labelling is misleading.
- **Findings are static-analysis output.** A finding states that a construct is present; it is not a proof of exploitability. False positives and missed issues are quality bugs, not security vulnerabilities.
- **Rules are not sandboxed.** Custom rule files are YAML evaluated by the engine's own rule compiler; a malformed rule fails the scan with an error rather than executing code.
- **Provider endpoints are user-controlled.** Requests go only to the base URL you configure; behaviour of third-party endpoints is out of scope.

## Security-relevant design decisions

- Scanning a workspace is **read-only**. Nothing is written into the scanned workspace; exports require an explicit user action.
- API keys are stored in a Rust-owned SQLite `secrets` table. No command returns key material, and keys never appear in logs, findings, scan events, errors, or exports.
- SARIF uploads read the token only from the `GITHUB_TOKEN` environment variable; the CLI has no plaintext-token argument and never echoes the token.
- The desktop UI (WebView) never receives secret plaintext and never opens the SQLite database directly.

## Supported versions

Only the latest published release is supported. Fixes land on `main` and ship in the next release.
