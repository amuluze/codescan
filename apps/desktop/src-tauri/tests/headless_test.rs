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
