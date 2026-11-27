//! CLI 集成测试：直接以子进程运行 `codescan`，覆盖退出码与输出契约。

use std::path::Path;
use std::process::{Command, Output};

fn codescan() -> Command {
    Command::new(env!("CARGO_BIN_EXE_codescan"))
}

fn run(args: &[&str]) -> Output {
    let data = tempfile::tempdir().unwrap();
    codescan()
        .env("CODESCAN_DATA_DIR", data.path())
        .args(args)
        .output()
        .expect("启动 codescan 失败")
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

fn write_file(root: &Path, rel: &str, content: &str) {
    let path = root.join(rel);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, content).unwrap();
}

#[test]
fn clean_directory_exits_zero_with_summary() {
    let dir = tempfile::tempdir().unwrap();
    let output = run(&["scan", dir.path().to_str().unwrap()]);

    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    let out = stdout(&output);
    assert!(out.contains("files="), "stdout: {out}");
    assert!(out.contains("findings=0"), "stdout: {out}");
    assert!(out.contains("events="), "stdout: {out}");
}

#[test]
fn todo_finding_text_and_severity_gates() {
    let dir = tempfile::tempdir().unwrap();
    write_file(dir.path(), "src/a.ts", "// TODO: x\nconst a = 1;\n");
    let root = dir.path().to_str().unwrap();

    let output = run(&["scan", root]);
    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    assert!(stdout(&output).contains("text/todo-debt"));

    // info 级发现不触发 error / warning 门禁。
    let output = run(&["scan", root, "--fail-on", "error"]);
    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));

    let output = run(&["scan", root, "--fail-on", "warning"]);
    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));

    // new 必须带 baseline。
    let output = run(&["scan", root, "--fail-on", "new"]);
    assert_eq!(output.status.code(), Some(2), "stdout: {}", stdout(&output));
}

#[test]
fn baseline_gate_detects_only_new_findings() {
    let dir = tempfile::tempdir().unwrap();
    write_file(dir.path(), "src/a.ts", "// TODO: x\nconst a = 1;\n");
    let root = dir.path().to_str().unwrap();
    let baseline = dir.path().join("baseline.json");
    let baseline_arg = baseline.to_str().unwrap();

    let output = run(&["scan", root, "--save-baseline", baseline_arg]);
    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    assert!(baseline.is_file());

    let output = run(&["scan", root, "--baseline", baseline_arg, "--fail-on", "new"]);
    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));

    // 新增一个 TODO 文件 → 基线外发现 → 门禁命中。
    write_file(dir.path(), "src/b.ts", "// TODO: y\n");
    let output = run(&["scan", root, "--baseline", baseline_arg, "--fail-on", "new"]);
    assert_eq!(output.status.code(), Some(1), "stdout: {}", stdout(&output));
}

#[test]
fn sarif_written_to_out_file() {
    let dir = tempfile::tempdir().unwrap();
    write_file(dir.path(), "src/a.ts", "// TODO: x\n");
    let sarif_path = dir.path().join("report.sarif");
    let output = run(&[
        "scan",
        dir.path().to_str().unwrap(),
        "--format",
        "sarif",
        "--out",
        sarif_path.to_str().unwrap(),
    ]);

    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    assert!(stdout(&output).contains("findings="));
    let content = std::fs::read_to_string(&sarif_path).unwrap();
    assert!(content.contains("\"version\": \"2.1.0\""), "{content}");
    assert!(content.contains("\"results\""), "{content}");
}

#[test]
fn invalid_rules_yaml_exits_two_with_stderr() {
    let dir = tempfile::tempdir().unwrap();
    write_file(dir.path(), "src/a.ts", "const a = 1;\n");
    let rules = dir.path().join("rules.yaml");
    std::fs::write(&rules, "rules: [oops\n").unwrap();

    let output = run(&[
        "scan",
        dir.path().to_str().unwrap(),
        "--rules",
        rules.to_str().unwrap(),
    ]);
    assert_eq!(output.status.code(), Some(2));
    assert!(!stderr(&output).trim().is_empty(), "stderr 应有错误文本");
}

#[test]
fn global_ignore_excludes_todo_findings() {
    let dir = tempfile::tempdir().unwrap();
    write_file(dir.path(), "generated/a.ts", "// TODO: generated\n");
    write_file(dir.path(), "src/b.ts", "const b = 1;\n");
    let ignore = dir.path().join("global.ignore");
    std::fs::write(&ignore, "generated/\n").unwrap();

    let output = run(&[
        "scan",
        dir.path().to_str().unwrap(),
        "--global-ignore",
        ignore.to_str().unwrap(),
    ]);

    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    let out = stdout(&output);
    assert!(!out.contains("text/todo-debt"), "stdout: {out}");
    assert!(!out.contains("generated/"), "stdout: {out}");
}

// ---------- 本地规则包 ----------

#[test]
fn pack_install_list_scan_and_remove() {
    let data = tempfile::tempdir().unwrap();
    let source = tempfile::tempdir().unwrap();
    write_file(
        source.path(),
        "pack.yaml",
        "id: com.example.cli\nversion: 1.0.0\nrules:\n  - rules.yaml\n",
    );
    write_file(
        source.path(),
        "rules.yaml",
        "rules:\n  - id: pack/cli-marker\n    language: \"*\"\n    dimension: quality\n    severity: warning\n    message: CLI pack marker\n    text: PACK_CLI_MARKER\n",
    );
    let data_dir = data.path().to_str().unwrap();

    let output = codescan()
        .env("CODESCAN_DATA_DIR", data_dir)
        .args(["pack", "install", source.path().to_str().unwrap()])
        .output()
        .expect("启动 codescan 失败");
    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    assert!(stdout(&output).contains("installed com.example.cli@1.0.0"), "{}", stdout(&output));

    let output = codescan()
        .env("CODESCAN_DATA_DIR", data_dir)
        .args(["pack", "list", "--json"])
        .output()
        .expect("启动 codescan 失败");
    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    let packs: serde_json::Value = serde_json::from_str(&stdout(&output)).unwrap();
    let pack = &packs.as_array().unwrap()[0];
    assert_eq!(pack["id"], "com.example.cli");
    assert_eq!(pack["version"], "1.0.0");
    assert_eq!(pack["error"], serde_json::Value::Null);
    assert_eq!(pack["integrity"].as_str().unwrap().len(), 64);

    let workspace = tempfile::tempdir().unwrap();
    write_file(workspace.path(), "src/a.ts", "// PACK_CLI_MARKER\nconst a = 1;\n");
    let root = workspace.path().to_str().unwrap();
    let output = codescan()
        .env("CODESCAN_DATA_DIR", data_dir)
        .args(["scan", root])
        .output()
        .expect("启动 codescan 失败");
    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    assert!(stdout(&output).contains("pack/cli-marker"), "{}", stdout(&output));

    let output = codescan()
        .env("CODESCAN_DATA_DIR", data_dir)
        .args(["pack", "remove", "com.example.cli", "1.0.0"])
        .output()
        .expect("启动 codescan 失败");
    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));

    let output = codescan()
        .env("CODESCAN_DATA_DIR", data_dir)
        .args(["scan", root])
        .output()
        .expect("启动 codescan 失败");
    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    assert!(!stdout(&output).contains("pack/cli-marker"), "{}", stdout(&output));
}

#[test]
fn pack_signing_trust_workflow() {
    let data = tempfile::tempdir().unwrap();
    let source = tempfile::tempdir().unwrap();
    let keys = tempfile::tempdir().unwrap();
    write_file(
        source.path(),
        "pack.yaml",
        "id: com.example.signed\nversion: 1.0.0\nrules:\n  - rules.yaml\n",
    );
    write_file(
        source.path(),
        "rules.yaml",
        "rules:\n  - id: pack/signed-marker\n    language: \"*\"\n    dimension: quality\n    severity: warning\n    message: Signed pack marker\n    text: SIGNED_PACK_MARKER\n",
    );
    let data_dir = data.path().to_str().unwrap();

    let output = codescan()
        .env("CODESCAN_DATA_DIR", data_dir)
        .args(["pack", "keygen", "--out", keys.path().to_str().unwrap(), "--json"])
        .output()
        .expect("启动 codescan 失败");
    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    let generated: serde_json::Value = serde_json::from_str(&stdout(&output)).unwrap();
    let key_id = generated["key_id"].as_str().unwrap();
    assert!(keys.path().join("private.key").is_file());
    assert!(keys.path().join("public.key").is_file());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(keys.path().join("private.key"))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
    }

    let output = codescan()
        .env("CODESCAN_DATA_DIR", data_dir)
        .args([
            "pack",
            "trust",
            "add",
            keys.path().join("public.key").to_str().unwrap(),
            "--json",
        ])
        .output()
        .expect("启动 codescan 失败");
    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));

    let output = codescan()
        .env("CODESCAN_DATA_DIR", data_dir)
        .args([
            "pack",
            "sign",
            source.path().to_str().unwrap(),
            "--key",
            keys.path().join("private.key").to_str().unwrap(),
            "--json",
        ])
        .output()
        .expect("启动 codescan 失败");
    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    let signature: serde_json::Value = serde_json::from_str(&stdout(&output)).unwrap();
    assert_eq!(signature["key_id"], key_id);
    assert!(source.path().join(".signature").is_file());

    let output = codescan()
        .env("CODESCAN_DATA_DIR", data_dir)
        .args(["pack", "install", source.path().to_str().unwrap()])
        .output()
        .expect("启动 codescan 失败");
    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));

    let output = codescan()
        .env("CODESCAN_DATA_DIR", data_dir)
        .args(["pack", "list", "--json"])
        .output()
        .expect("启动 codescan 失败");
    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    let packs: serde_json::Value = serde_json::from_str(&stdout(&output)).unwrap();
    assert_eq!(packs[0]["signed_by"], key_id);
    assert_eq!(packs[0]["error"], serde_json::Value::Null);

    let workspace = tempfile::tempdir().unwrap();
    write_file(
        workspace.path(),
        "src/a.ts",
        "// SIGNED_PACK_MARKER\nconst a = 1;\n",
    );
    let output = codescan()
        .env("CODESCAN_DATA_DIR", data_dir)
        .args(["scan", workspace.path().to_str().unwrap()])
        .output()
        .expect("启动 codescan 失败");
    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    assert!(stdout(&output).contains("pack/signed-marker"), "{}", stdout(&output));

    // 配置信任公钥后，未签名包被拒绝。
    let unsigned = tempfile::tempdir().unwrap();
    write_file(
        unsigned.path(),
        "pack.yaml",
        "id: com.example.unsigned\nversion: 1.0.0\nrules:\n  - rules.yaml\n",
    );
    write_file(
        unsigned.path(),
        "rules.yaml",
        "rules:\n  - id: pack/unsigned-marker\n    language: \"*\"\n    dimension: quality\n    severity: warning\n    message: Unsigned marker\n    text: UNSIGNED_PACK_MARKER\n",
    );
    let output = codescan()
        .env("CODESCAN_DATA_DIR", data_dir)
        .args(["pack", "install", unsigned.path().to_str().unwrap()])
        .output()
        .expect("启动 codescan 失败");
    assert_eq!(output.status.code(), Some(2));
    assert!(stderr(&output).contains("未签名"), "{}", stderr(&output));
}

#[test]
fn pack_rejects_invalid_source() {
    let data = tempfile::tempdir().unwrap();
    let source = tempfile::tempdir().unwrap();
    write_file(
        source.path(),
        "pack.yaml",
        "id: ../escape\nversion: 1.0.0\nrules:\n  - rules.yaml\n",
    );
    let output = codescan()
        .env("CODESCAN_DATA_DIR", data.path())
        .args(["pack", "install", source.path().to_str().unwrap()])
        .output()
        .expect("启动 codescan 失败");
    assert_eq!(output.status.code(), Some(2));
    assert!(stderr(&output).contains("规则包"), "{}", stderr(&output));
}

// ---------- v6.1：GitHub Code Scanning 上传 ----------

const SHA: &str = "0123456789abcdef0123456789abcdef01234567";

fn write_sarif(dir: &Path, name: &str, content: &str) -> String {
    write_file(dir, name, content);
    dir.join(name).to_str().unwrap().to_string()
}

#[test]
fn upload_dry_run_reports_metadata_without_network() {
    let dir = tempfile::tempdir().unwrap();
    let sarif = write_sarif(dir.path(), "out.sarif", "{\"version\":\"2.1.0\",\"runs\":[]}");
    let output = run(&[
        "github-upload",
        "--sarif",
        &sarif,
        "--repo",
        "owner/repo",
        "--commit",
        SHA,
        "--dry-run",
    ]);
    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    let out = stdout(&output);
    assert!(out.contains("endpoint=https://api.github.com/repos/owner/repo/code-scanning/sarifs"), "{out}");
    assert!(out.contains("payload_bytes="), "{out}");
    assert!(out.contains("sarif_bytes="), "{out}");
    assert!(out.contains("token_present=false"), "{out}");
}

#[test]
fn upload_uses_ci_env_fallbacks() {
    let dir = tempfile::tempdir().unwrap();
    let sarif = write_sarif(dir.path(), "out.sarif", "{\"version\":\"2.1.0\",\"runs\":[]}");
    let output = codescan()
        .args(["github-upload", "--sarif", &sarif, "--dry-run"])
        .env("GITHUB_REPOSITORY", "env/repo")
        .env("GITHUB_SHA", SHA)
        .env("GITHUB_REF", "refs/heads/dev")
        .env_remove("GITHUB_TOKEN")
        .output()
        .expect("启动 codescan 失败");
    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    let out = stdout(&output);
    assert!(out.contains("endpoint=https://api.github.com/repos/env/repo/code-scanning/sarifs"), "{out}");
    assert!(out.contains("ref=refs/heads/dev"), "{out}");
}

#[test]
fn upload_rejects_missing_token_repo_commit_and_bad_sarif() {
    let dir = tempfile::tempdir().unwrap();
    let sarif = write_sarif(dir.path(), "out.sarif", "{\"version\":\"2.1.0\",\"runs\":[]}");

    // 缺 token（非 dry-run）。
    let output = codescan()
        .args([
            "github-upload", "--sarif", &sarif, "--repo", "owner/repo", "--commit", SHA,
        ])
        .env_remove("GITHUB_TOKEN")
        .output()
        .expect("启动 codescan 失败");
    assert_eq!(output.status.code(), Some(2));
    assert!(stderr(&output).contains("GITHUB_TOKEN"), "{}", stderr(&output));

    // 非法 repo。
    let output = run(&[
        "github-upload", "--sarif", &sarif, "--repo", "bad", "--commit", SHA, "--dry-run",
    ]);
    assert_eq!(output.status.code(), Some(2));
    assert!(stderr(&output).contains("repo"), "{}", stderr(&output));

    // 非法 commit。
    let output = run(&[
        "github-upload", "--sarif", &sarif, "--repo", "owner/repo", "--commit", "abc", "--dry-run",
    ]);
    assert_eq!(output.status.code(), Some(2));
    assert!(stderr(&output).contains("commit"), "{}", stderr(&output));

    // SARIF 版本错误。
    let bad = write_sarif(dir.path(), "bad.sarif", "{\"version\":\"1.0\",\"runs\":[]}");
    let output = run(&[
        "github-upload", "--sarif", &bad, "--repo", "owner/repo", "--commit", SHA, "--dry-run",
    ]);
    assert_eq!(output.status.code(), Some(2));
    assert!(stderr(&output).contains("2.1.0"), "{}", stderr(&output));

    // 缺失文件。
    let output = run(&[
        "github-upload", "--sarif", "/nonexistent/x.sarif", "--repo", "owner/repo",
        "--commit", SHA, "--dry-run",
    ]);
    assert_eq!(output.status.code(), Some(2));
}
