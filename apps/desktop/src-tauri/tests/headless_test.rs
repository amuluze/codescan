//! A8（Rust 侧）+ A11 无头测试：Store 往返、基线差异、取消不留历史。
//! 不启动 Tauri；引擎扫描直接经 ScanManager。

use codescan_lib::manager::ScanManager;
use codescan_lib::store::Store;
use codescan_engine::model::{Dimension, Finding, Position, ScanReport, Severity, Span};
use std::sync::{Arc, Mutex};

fn finding(rule: &str, path: &str, line: usize, fingerprint: &str) -> Finding {
    Finding {
        rule_id: rule.into(),
        dimension: Dimension::Quality,
        severity: Severity::Warning,
        path: path.into(),
        span: Span {
            start: Position { line, column: 1 },
            end: Position { line, column: 9 },
        },
        message: "m".into(),
        match_text: "x".into(),
        captures: Vec::new(),
        degraded: false,
        flows: Vec::new(),
        fingerprint: fingerprint.into(),
    }
}

fn report(findings: Vec<Finding>) -> ScanReport {
    ScanReport {
        engine_version: "0.1.0".into(),
        findings: findings.clone(),
        events: Vec::new(),
        stats: codescan_engine::model::ScanStats {
            findings: findings.len() as u64,
            ..Default::default()
        },
    }
}

#[test]
fn store_roundtrip_and_baseline_gate() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(&dir.path().join("test.db")).unwrap();

    // 指纹需唯一稳定：借用引擎 finalize 生成。
    let f1 = finding("r/a", "src/a.ts", 1, "fp-a");
    let f2 = finding("r/b", "src/b.py", 5, "fp-b");
    let scan_id = store
        .insert_scan("/tmp/ws", &report(vec![f1.clone(), f2.clone()]), 12, 1_000)
        .unwrap();

    let summaries = store.list_scans().unwrap();
    assert_eq!(summaries.len(), 1);
    assert_eq!(summaries[0].id, scan_id);
    assert_eq!(summaries[0].findings, 2);
    assert_eq!(summaries[0].files_scanned, 0);

    let got = store.get_findings(scan_id).unwrap();
    assert_eq!(got.len(), 2);
    assert_eq!(got[0].fingerprint, "fp-a");
    assert_eq!(got[0].span.start.line, 1);
    assert_eq!(got[0].dimension, Dimension::Quality);
    assert_eq!(got[0].severity, Severity::Warning);

    // 基线：包含两条；新扫描（fp-c 新增、fp-a 修复）→ 门禁差 = [fp-c]。
    store.set_baseline(scan_id, 2_000).unwrap();
    let baseline = store.current_baseline().unwrap().unwrap();
    assert_eq!(baseline.entries.len(), 2);

    let scan_id2 = store
        .insert_scan(
            "/tmp/ws",
            &report(vec![
                finding("r/a", "src/a.ts", 1, "fp-c"),
                finding("r/b", "src/b.py", 5, "fp-b"),
            ]),
            9,
            3_000,
        )
        .unwrap();
    let gate = store.gate_new(scan_id2).unwrap().unwrap();
    assert_eq!(gate.len(), 1);
    assert_eq!(gate[0].fingerprint, "fp-c");

    // 跨连接（重新打开）持久可读。
    drop(store);
    let store2 = Store::open(&dir.path().join("test.db")).unwrap();
    assert_eq!(store2.get_findings(scan_id).unwrap().len(), 2);
    assert!(store2.current_baseline().unwrap().is_some());

    // 不存在的扫描 → 结构化错误。
    assert!(store2.get_findings(999).is_err());
}

#[test]
fn rule_state_persists() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(&dir.path().join("test.db")).unwrap();
    store.set_rule_enabled("ts/no-eval", false).unwrap();
    store.set_rule_enabled("py/no-eval-exec", true).unwrap();
    let disabled = store.disabled_rules().unwrap();
    assert_eq!(disabled, vec!["ts/no-eval".to_string()]);
}

/// A11：预置取消令牌 → 扫描取消 → 历史无新记录。
#[test]
fn a11_cancelled_scan_leaves_no_history() {
    let dir = tempfile::tempdir().unwrap();
    let ws = dir.path().join("ws");
    std::fs::create_dir_all(&ws).unwrap();
    for i in 0..50 {
        std::fs::write(ws.join(format!("f{i}.ts")), format!("const v{i} = {i};\n")).unwrap();
    }
    let store = Arc::new(Mutex::new(
        Store::open(&dir.path().join("test.db")).unwrap(),
    ));
    let manager = ScanManager::new(store.clone());

    let cancel = codescan_engine::CancelToken::new();
    cancel.cancel();
    let outcome = manager.run_scan_with_token(&ws, None, &cancel, &|_| {});
    assert!(matches!(
        outcome,
        Err(codescan_lib::manager::ManagerError::Scan(
            codescan_engine::ScanError::Cancelled
        ))
    ));
    assert!(store.lock().unwrap().list_scans().unwrap().is_empty());
}

/// 正常扫描落历史且返回发现。
#[test]
fn successful_scan_persists() {
    let dir = tempfile::tempdir().unwrap();
    let ws = dir.path().join("ws");
    std::fs::create_dir_all(&ws).unwrap();
    std::fs::write(ws.join("a.ts"), "// TODO: debt\nconst a = 1;\n").unwrap();

    let store = Arc::new(Mutex::new(
        Store::open(&dir.path().join("test.db")).unwrap(),
    ));
    let manager = ScanManager::new(store.clone());
    let outcome = manager.run_scan(&ws, None, &|_| {}).unwrap();
    assert_eq!(outcome.report.stats.files_scanned, 1);
    assert!(outcome
        .report
        .findings
        .iter()
        .any(|f| f.rule_id == "text/todo-debt"));
    assert_eq!(store.lock().unwrap().list_scans().unwrap().len(), 1);
}

// ---------- v3：LLM 辅助层（Domain §3a） ----------

use codescan_lib::commands::{prepare_batch_llm_call, prepare_llm_call, prepare_provider_probe};
use codescan_lib::llm;

#[test]
fn c1_secrets_boundary_roundtrip() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("test.db");
    let store = Store::open(&db).unwrap();
    assert!(!store.has_secret(llm::SECRET_API_KEY).unwrap());

    store.set_secret(llm::SECRET_API_KEY, "sk-secret").unwrap();
    assert!(store.has_secret(llm::SECRET_API_KEY).unwrap());
    // Rust 内部可读；命令面只暴露 has_secret（结构保证，见 commands.rs）。
    assert_eq!(
        store.get_secret(llm::SECRET_API_KEY).unwrap().as_deref(),
        Some("sk-secret")
    );

    store.delete_secret(llm::SECRET_API_KEY).unwrap();
    assert!(!store.has_secret(llm::SECRET_API_KEY).unwrap());

    // 持久化跨连接。
    store.set_secret(llm::SECRET_API_KEY, "sk-2").unwrap();
    drop(store);
    let store2 = Store::open(&db).unwrap();
    assert!(store2.has_secret(llm::SECRET_API_KEY).unwrap());
}

#[test]
fn c2_provider_config_roundtrip_and_missing_key() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(&dir.path().join("test.db")).unwrap();
    let config = llm::ProviderConfig {
        kind: llm::ProviderKind::Anthropic,
        base_url: "https://api.anthropic.com".into(),
        model: "claude-x".into(),
    };
    store.set_meta(llm::META_PROVIDER, &config.to_json()).unwrap();
    let loaded = llm::ProviderConfig::from_json(
        &store.get_meta(llm::META_PROVIDER).unwrap().unwrap(),
    )
    .unwrap();
    assert_eq!(loaded, config);

    // 无密钥 → 可读错误，且不发起任何请求（错误在 prepare 阶段返回）。
    let scan_id = store
        .insert_scan("/tmp/ws", &report(vec![finding("r/a", "a.ts", 1, "fp-a")]), 1, 1)
        .unwrap();
    let err = prepare_llm_call(&store, None, scan_id, "fp-a").unwrap_err();
    assert!(err.contains("API Key"), "错误应提示未配置密钥：{err}");
    assert!(!err.contains("sk-"), "错误不得包含密钥");

    // 缺 finding / 非法配置同样在 prepare 阶段被拒。
    store.set_secret(llm::SECRET_API_KEY, "sk-test").unwrap();
    assert!(prepare_llm_call(&store, None, scan_id, "missing-fp").is_err());
    store
        .set_meta(
            llm::META_PROVIDER,
            &llm::ProviderConfig {
                kind: llm::ProviderKind::OpenAi,
                base_url: "https://api.openai.com/v1".into(),
                model: String::new(),
            }
            .to_json(),
        )
        .unwrap();
    assert!(prepare_llm_call(&store, None, scan_id, "fp-a").is_err());
}

#[test]
fn c7_llm_failure_is_recoverable() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(&dir.path().join("test.db")).unwrap();
    let scan_id = store
        .insert_scan("/tmp/ws", &report(vec![finding("r/a", "a.ts", 1, "fp-a")]), 1, 1)
        .unwrap();
    store.set_secret(llm::SECRET_API_KEY, "sk-test").unwrap();
    store
        .set_meta(
            llm::META_PROVIDER,
            &llm::ProviderConfig {
                kind: llm::ProviderKind::OpenAi,
                // 指向必拒绝的本地端口，验证网络错误可恢复且不 panic。
                base_url: "http://127.0.0.1:9/v1".into(),
                model: "m".into(),
            }
            .to_json(),
        )
        .unwrap();

    let (config, key, context) = prepare_llm_call(&store, None, scan_id, "fp-a").unwrap();
    let (system, user) = llm::triage_prompt(&context);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let result = runtime.block_on(llm::complete(&config, &key, &system, &user));
    assert!(result.is_err(), "网络失败应返回错误：{result:?}");
    let err = result.err().unwrap();
    assert!(!err.contains("sk-test"), "错误不得包含密钥：{err}");

    // 流式路径同样可恢复且不泄密。
    let stream_result = runtime.block_on(llm::complete_stream(
        &config,
        &key,
        &system,
        &user,
        |_| {},
    ));
    assert!(stream_result.is_err(), "流式网络失败应返回错误：{stream_result:?}");
    let err = stream_result.err().unwrap();
    assert!(!err.contains("sk-test"), "流式错误不得包含密钥：{err}");
    // 存储与基线不受影响。
    assert_eq!(store.list_scans().unwrap().len(), 1);
    assert!(store.current_baseline().unwrap().is_none());
}

#[test]
fn c8_batch_triage_prepare_and_parse_are_read_only() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(&dir.path().join("test.db")).unwrap();
    let scan_id = store
        .insert_scan(
            "/tmp/ws",
            &report(vec![
                finding("r/a", "a.ts", 1, "fp-a"),
                finding("r/b", "b.ts", 3, "fp-b"),
            ]),
            2,
            1,
        )
        .unwrap();
    store.set_baseline(scan_id, 2).unwrap();
    store.set_secret(llm::SECRET_API_KEY, "sk-test").unwrap();
    store
        .set_meta(
            llm::META_PROVIDER,
            &llm::ProviderConfig {
                kind: llm::ProviderKind::OpenAi,
                base_url: "https://api.openai.com/v1".into(),
                model: "m".into(),
            }
            .to_json(),
        )
        .unwrap();

    let before_findings = store.get_findings(scan_id).unwrap();
    let before_baseline = store.current_baseline().unwrap();
    let before_gate = store.gate_new(scan_id).unwrap();

    // 传空指纹 → 后端选取全部（当前两条）并构建提示词。
    let (config, key, selected) =
        prepare_batch_llm_call(&store, scan_id, &[]).unwrap();
    assert!(config.validate().is_ok());
    assert!(!key.is_empty());
    let expected: Vec<String> = selected.iter().map(|f| f.fingerprint.clone()).collect();
    assert_eq!(expected, vec!["fp-a", "fp-b"]);

    let (system, user) = llm::batch_triage_prompt(&selected);
    assert!(system.contains("JSON 数组"));
    assert!(user.contains("fp-a") && user.contains("fp-b"));
    let bad = r#"[
      {"fingerprint":"fp-a","assessment":"false-positive","rationale":"x"},
      {"fingerprint":"fp-b","assessment":"likely-false-positive","rationale":"y"}
    ]"#;
    // 非法 assessment → 可读错误，不产生部分结果。
    assert!(llm::parse_batch_triage(bad, &expected).is_err());

    let good = r#"[{"fingerprint":"fp-a","assessment":"uncertain","rationale":"x"}]"#;
    let parsed = llm::parse_batch_triage(good, &expected).unwrap();
    assert_eq!(parsed[0].fingerprint, "fp-a");
    assert!(!parsed[0].missing);
    assert!(parsed[1].missing);

    // 显式超过上限 → 可读错误（无需 Provider 成功）。
    let too_many: Vec<String> = (0..=llm::BATCH_TRIAGE_LIMIT)
        .map(|i| format!("fp-{i}"))
        .collect();
    let err = prepare_batch_llm_call(&store, scan_id, &too_many).unwrap_err();
    assert!(err.contains("20"), "{err}");

    // 调用前后 findings / 基线 / 门禁差异完全不变。
    assert_eq!(store.get_findings(scan_id).unwrap(), before_findings);
    assert_eq!(store.current_baseline().unwrap(), before_baseline);
    assert_eq!(store.gate_new(scan_id).unwrap(), before_gate);
}

// ---------- v3.3：候选规则隔离回归（G1–G5） ----------

use codescan_lib::regression::{run_from_store, RegressionActual};

const HIT_RULE: &str = r#"rules:
  - id: custom/regression-eval
    language: "*"
    dimension: security
    severity: warning
    message: eval 调用
    text: 'eval\('
"#;

const MISS_RULE: &str = r#"rules:
  - id: custom/regression-eval
    language: "*"
    dimension: security
    severity: warning
    message: eval 调用
    text: 'alert\('
"#;

fn regression_fixture() -> (tempfile::TempDir, Store, i64) {
    let dir = tempfile::tempdir().unwrap();
    let ws = dir.path().join("ws");
    std::fs::create_dir_all(ws.join("src")).unwrap();
    std::fs::write(ws.join("src/a.ts"), "const x = 1;\neval(userInput);\n").unwrap();
    let store = Store::open(&dir.path().join("test.db")).unwrap();
    let scan_id = store
        .insert_scan(
            ws.to_str().unwrap(),
            &report(vec![finding("ts/no-eval", "src/a.ts", 2, "fp-eval")]),
            5,
            1,
        )
        .unwrap();
    (dir, store, scan_id)
}

/// G1：命中种子行的候选 → pass=true，actual 含期望行；可保存夹具。
#[test]
fn v33_regression_hit_passes_and_saves_fixture() {
    let (dir, store, scan_id) = regression_fixture();
    let data_root = dir.path().join(".codescan");
    let result = run_from_store(&store, &data_root, scan_id, "fp-eval", HIT_RULE, true).unwrap();

    assert!(result.valid);
    assert!(result.pass, "failures: {:?}", result.failures);
    assert_eq!(result.expected, vec![3]);
    assert_eq!(
        result.actual,
        vec![RegressionActual {
            line: 3,
            message: "eval 调用".into(),
        }]
    );
    assert!(result.failures.is_empty());

    // 夹具写入 `<data_root>/rules/tests/<安全化规则 id>/<文件名>`。
    assert_eq!(result.saved_paths.len(), 1);
    let saved_path = &result.saved_paths[0];
    assert!(
        saved_path.contains("rules/tests/custom_regression-eval/a.ts"),
        "{saved_path}"
    );
    let saved = std::fs::read_to_string(saved_path).unwrap();
    assert!(saved.contains("// ruleid: custom/regression-eval"));
    assert!(saved.contains("eval(userInput);"));
}

/// G2：不命中的候选 → pass=false，failures 可读。
#[test]
fn v33_regression_miss_fails_with_readable_reason() {
    let (dir, store, scan_id) = regression_fixture();
    let result = run_from_store(
        &store,
        &dir.path().join(".codescan"),
        scan_id,
        "fp-eval",
        MISS_RULE,
        false,
    )
    .unwrap();

    assert!(result.valid);
    assert!(!result.pass);
    assert!(result.actual.is_empty());
    assert_eq!(result.expected, vec![3]);
    assert!(
        result
            .failures
            .iter()
            .any(|f| f.contains("漏报") && f.contains("custom/regression-eval")),
        "{:?}",
        result.failures
    );
    assert!(result.saved_paths.is_empty());
}

/// G3：非法候选返回 valid=false（先于存储访问）；源文件缺失返回可读错误。
#[test]
fn v33_regression_rejects_invalid_candidate_and_missing_source() {
    let (dir, store, _scan_id) = regression_fixture();
    let invalid = r#"rules:
  - id: ts/not-custom
    language: "*"
    dimension: security
    severity: warning
    message: m
    text: 'eval\('
"#;
    // 候选非法时不读取不存在的扫描记录，也不 panic。
    let result =
        run_from_store(&store, &dir.path().join(".codescan"), 999, "nope", invalid, false).unwrap();
    assert!(!result.valid);
    assert!(!result.pass);
    assert!(
        result.errors.iter().any(|e| e.contains("custom/")),
        "{:?}",
        result.errors
    );

    // 有效候选 + 缺失源文件 → 可读错误。
    let ws = dir.path().join("ws");
    let missing_id = store
        .insert_scan(
            ws.to_str().unwrap(),
            &report(vec![finding("ts/x", "src/missing.ts", 1, "fp-missing")]),
            1,
            2,
        )
        .unwrap();
    let err = run_from_store(
        &store,
        &dir.path().join(".codescan"),
        missing_id,
        "fp-missing",
        HIT_RULE,
        false,
    )
    .unwrap_err();
    assert!(err.contains("读取源文件失败"), "{err}");
}

/// G5：调用前后 findings/基线/历史完全不变（只读纪律）。
#[test]
fn v33_regression_is_read_only() {
    let (dir, store, scan_id) = regression_fixture();
    store.set_baseline(scan_id, 2).unwrap();
    let before_findings = store.get_findings(scan_id).unwrap();
    let before_baseline = store.current_baseline().unwrap();
    let before_scans = store.list_scans().unwrap();
    let before_gate = store.gate_new(scan_id).unwrap();

    let result = run_from_store(
        &store,
        &dir.path().join(".codescan"),
        scan_id,
        "fp-eval",
        HIT_RULE,
        true,
    )
    .unwrap();
    assert!(result.pass);

    assert_eq!(store.get_findings(scan_id).unwrap(), before_findings);
    assert_eq!(store.current_baseline().unwrap(), before_baseline);
    let after_scans = store.list_scans().unwrap();
    assert_eq!(after_scans.len(), before_scans.len());
    assert_eq!(after_scans[0].id, before_scans[0].id);
    assert_eq!(after_scans[0].root, before_scans[0].root);
    assert_eq!(store.gate_new(scan_id).unwrap(), before_gate);
}

/// 目录选择器后端：仅列出直属子目录、跳过隐藏、排序确定、父目录可回退。
#[test]
fn list_directory_lists_only_visible_subdirectories() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    std::fs::create_dir_all(root.join("alpha")).unwrap();
    std::fs::create_dir_all(root.join("Beta")).unwrap();
    std::fs::create_dir_all(root.join(".hidden")).unwrap();
    std::fs::write(root.join("file.txt"), "x").unwrap();

    let listing = codescan_lib::commands::list_directory(Some(
        root.to_string_lossy().into_owned(),
    ))
    .expect("列目录失败");
    let names: Vec<&str> = listing.entries.iter().map(|e| e.name.as_str()).collect();
    assert_eq!(names, vec!["alpha", "Beta"], "只列可见目录并按名称排序");
    assert_eq!(
        listing.path,
        root.canonicalize().unwrap().to_string_lossy()
    );
    assert!(listing.parent.is_some() || root.parent().is_none());

    // 非法路径可读报错。
    assert!(codescan_lib::commands::list_directory(Some(
        root.join("missing").to_string_lossy().into_owned()
    ))
    .is_err());
}

/// 符号链接目录（如 macOS /tmp → private/tmp）必须可选。
#[cfg(unix)]
#[test]
fn list_directory_follows_directory_symlinks() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    std::fs::create_dir_all(root.join("real")).unwrap();
    std::os::unix::fs::symlink(root.join("real"), root.join("linked")).unwrap();
    std::fs::write(root.join("plain.txt"), "x").unwrap();

    let listing = codescan_lib::commands::list_directory(Some(
        root.to_string_lossy().into_owned(),
    ))
    .unwrap();
    let names: Vec<&str> = listing.entries.iter().map(|e| e.name.as_str()).collect();
    assert_eq!(names, vec!["linked", "real"]);
}

/// 测试连接的前置校验：配置不完整或缺少 Key 时不发起网络请求。
#[test]
fn provider_probe_preflight_rejects_incomplete_config() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(&dir.path().join("test.db")).unwrap();

    // 默认配置 model 为空。
    let err = prepare_provider_probe(&store).unwrap_err();
    assert!(err.contains("model"), "应提示 model 缺失：{err}");

    store
        .set_meta(
            llm::META_PROVIDER,
            &llm::ProviderConfig {
                kind: llm::ProviderKind::OpenAi,
                base_url: "https://api.openai.com/v1".into(),
                model: "gpt-test".into(),
            }
            .to_json(),
        )
        .unwrap();
    let err = prepare_provider_probe(&store).unwrap_err();
    assert!(err.contains("API Key"), "应提示未配置 Key：{err}");

    store.set_secret(llm::SECRET_API_KEY, "sk-probe").unwrap();
    let (config, key) = prepare_provider_probe(&store).unwrap();
    assert_eq!(config.model, "gpt-test");
    assert_eq!(key, "sk-probe");
}
