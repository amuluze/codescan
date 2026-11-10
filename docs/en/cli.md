# codescan CLI (English)

`codescan` is the headless entry point to the CodeScan deterministic scanning engine, meant for local checks and CI gating. It shares the engine with the desktop app: same engine version, same rule set, same fingerprint and baseline semantics.

The [Chinese version of this document](../cli.md) is the original; this file mirrors it.

## Install / run

```bash
cargo run -p codescan-cli -- scan <root>
# or use the built binary: target/debug/codescan
```

## Commands

```text
codescan scan <root> [OPTIONS]
```

| Option | Meaning |
| --- | --- |
| `--format text\|json\|sarif` | Output format, default `text`. `json` is the `ScanReport`; `sarif` is SARIF 2.1.0. |
| `--out <path>` | Write the selected format to a file; stdout then prints a single summary line `files=X findings=Y events=Z`. Without it the output goes to stdout (for `text`, the first line is the summary). |
| `--rules <path>` | Custom rule YAML layered on top of the built-ins; a missing or non-compiling file exits with code `2`. |
| `--global-ignore <path>` | Additional global ignore file (gitignore syntax), combined with `.gitignore`, `.codescanignore`, and the built-in default directories. |
| `--baseline <path>` | Baseline JSON (`Baseline`); a missing or unparsable file exits with code `2`. |
| `--save-baseline <path>` | Write the fingerprint snapshot of the current findings as pretty JSON. |
| `--fail-on new\|error\|warning\|none` | Gate condition, default `none`. `new` requires `--baseline`, otherwise exit code `2`. |

### text output

```text
files=12 findings=2 events=0
src/a.ts:1:1 text/todo-debt info TODO/FIXME debt marker
```

Each finding line is `path:line:col rule_id severity message`; paths are workspace-relative and the order is deterministic.

## Exit codes

| Code | Meaning |
| --- | --- |
| `0` | Scan completed and the gate passed (or `--fail-on none`). |
| `1` | Gate hit: `error` / `warning` findings exist, or findings outside the baseline exist in `new` mode. |
| `2` | Usage or runtime error (invalid arguments, missing or unparsable rule/baseline file, scan IO error, …). The reason is written to stderr. |

## CI usage

Establish a baseline once and commit it:

```bash
codescan scan . --save-baseline baseline.json
```

Then let the pipeline block only newly introduced findings:

```bash
codescan scan . \
  --baseline baseline.json \
  --fail-on new \
  --format sarif \
  --out codescan.sarif
```

- Exit code `0`: no findings outside the baseline.
- Exit code `1`: new findings — the pipeline fails, and `codescan.sarif` can be uploaded to a code-scanning platform.

For a stricter, baseline-free gate use `--fail-on error` or `--fail-on warning`.

## SARIF upload (GitHub Code Scanning)

```bash
codescan github-upload --sarif codescan.sarif --repo owner/name \
  --commit "$GITHUB_SHA" --ref "$GITHUB_REF"
```

| Option | Meaning |
| --- | --- |
| `--sarif <path>` | SARIF 2.1.0 file (produced by `scan --format sarif --out`). |
| `--repo <owner/name>` | Repository; defaults to `GITHUB_REPOSITORY`. |
| `--commit <sha>` | Full 40-character commit SHA; defaults to `GITHUB_SHA`. |
| `--ref <ref>` | Git ref; defaults to `GITHUB_REF`, then falls back to `refs/heads/main`. |
| `--dry-run` | Print endpoint, commit, ref, payload size, and token presence without any network call. |

- The token is read **only** from the `GITHUB_TOKEN` environment variable (needs `security-events: write`); there is no plaintext-token argument.
- Request: `POST https://api.github.com/repos/{owner}/{repo}/code-scanning/sarifs`, `sarif` = `base64(gzip(SARIF))`, `tool_name = Codescan`.
- Exit codes: `0` upload succeeded; `2` argument/file/network/HTTP error (stderr explains why, never echoes the token).
- GitHub Actions example:

```yaml
permissions:
  contents: read
  security-events: write
steps:
  - run: codescan scan . --format sarif --out codescan.sarif
  - run: codescan github-upload --sarif codescan.sarif
```
