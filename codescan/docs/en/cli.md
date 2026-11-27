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
codescan pack install <source> [--json]
codescan pack list [--json]
codescan pack remove <id> <version> [--json]
codescan pack keygen --out <dir> [--json]
codescan pack sign <source> --key <private.key> [--json]
codescan pack trust add <public.key> [--json]
codescan pack trust list [--json]
codescan pack trust remove <key-id> [--json]
codescan github-upload --sarif <path> [OPTIONS]
```

The rule-pack data root defaults to `~/.codescan`; set `CODESCAN_DATA_DIR` to override it for all CLI commands. See [rules.md](rules.md#rule-packs-local-mvp) for the pack layout.

| Option | Meaning |
| --- | --- |
| `--format text\|json\|sarif` | Output format, default `text`. `json` is the `ScanReport`; `sarif` is SARIF 2.1.0. |
| `--out <path>` | Write the selected format to a file; stdout then prints a single summary line `files=X findings=Y events=Z`. Without it the output goes to stdout (for `text`, the first line is the summary). |
| `--rules <path>` | Custom rule YAML layered on top of the built-ins and installed packs; a missing or non-compiling file exits with code `2`. |
| Installed packs | Packs under `CODESCAN_DATA_DIR/packs` are loaded automatically; an invalid or tampered pack exits with code `2`. |
| `--global-ignore <path>` | Additional global ignore file (gitignore syntax), combined with `.gitignore`, `.codescanignore`, and the built-in default directories. |
| `--baseline <path>` | Baseline JSON (`Baseline`); a missing or unparsable file exits with code `2`. |
| `--save-baseline <path>` | Write the fingerprint snapshot of the current findings as pretty JSON. |
| `--fail-on new\|error\|warning\|none` | Gate condition, default `none`. `new` requires `--baseline`, otherwise exit code `2`. |

### Rule packs

```bash
codescan pack install ./my-pack
codescan pack list
codescan pack list --json
codescan pack remove com.example.rules 1.0.0
```

- `pack install` expects a source directory containing `pack.yaml`; it validates the manifest, path safety, engine range, rule compilation, and id conflicts, then writes `.integrity`.
- `pack list` prints id@version, rule count, and integrity; an invalid pack shows `error=...` and still exits `0` for inspection.
- `--json` emits the `RulePackInfo` array or the install/remove result for scripts.
- Pack command failures (missing source, invalid pack, duplicate install, illegal id/version, signature/trust failure) exit with code `2`.

### Signatures and trust

```bash
codescan pack keygen --out ./pack-keys
codescan pack trust add ./pack-keys/public.key
codescan pack sign ./my-pack --key ./pack-keys/private.key
codescan pack install ./my-pack
```

- `keygen` writes `private.key` (Ed25519 PKCS#8 base64, Unix mode 0600) and `public.key`; it refuses to overwrite existing files.
- `sign` writes `.signature` over the pack content hash; the algorithm is Ed25519.
- Trusted public keys live at `<data_root>/trusted-keys/<sha256>.pub`; `trust list` prints key ids.
- With no trusted keys, unsigned packs are allowed. Once trusted keys exist, every pack must be signed by a trusted key; install and scan fail otherwise.
- `key_id` is the SHA-256 of the public key (hex); `list --json` reports it as `signed_by`.

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
