//! 集成测试：对应 Task Spec A1/A3/A4/A5/A7/A9 与取消语义。
//! 语料在 tempfile 中构造，全流程走 Engine::scan。

use codescan_engine::{baseline, sarif, CancelToken, Engine, ScanEvent};
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};

fn write_file(root: &std::path::Path, rel: &str, content: &str) {
    let path = root.join(rel);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, content).unwrap();
}

/// 多形态语料：正常 TS/Python、语法错误、二进制、超限、未知扩展、忽略文件。
fn build_corpus(root: &std::path::Path) {
    write_file(
        root,
        "src/a.ts",
        "export function greet(name: string): string {\n  return 'hi ' + name;\n}\n",
    );
    write_file(
        root,
        "src/b.py",
        "def calc(x):\n    if x > 1:\n        return x * 2\n    return x\n",
    );
    // 污点流（过程内 param → eval + 同文件跨函数调用链）。
    write_file(
        root,
        "src/cross.ts",
        "function run(code: string): void {\n  eval(code);\n}\n\nfunction entry(userInput: string): void {\n  run(userInput);\n}\n",
    );
    // 污点流（过程内 param → eval）。
    write_file(
        root,
        "src/taint.ts",
        "function run(userInput: string): void {\n  eval(userInput);\n}\n",
    );
    // 语法错误（容错解析，命中需标 degraded）。
    write_file(
        root,
        "src/broken.ts",
        "function f( {\n  // TODO: 修复语法错误\n  return 1;\n",
    );
    // 二进制。
    let bin_path = root.join("assets/blob.bin");
    std::fs::create_dir_all(bin_path.parent().unwrap()).unwrap();
    std::fs::write(&bin_path, [0x00u8, 0x01, 0x02, 0x00, 0x03]).unwrap();
    // 超限（>1 MiB）。
    let big_path = root.join("src/huge.ts");
    std::fs::write(&big_path, vec![b'a'; 1024 * 1024 + 1]).unwrap();
    // 未知语言。
    write_file(root, "docs/readme.md", "# t\ntext\n");
    // gitignore 排除。
    write_file(root, ".gitignore", "ignored-dir/\n");
    write_file(root, "ignored-dir/skip.ts", "const a = 1;\n");
}

fn scan_root(root: &std::path::Path) -> codescan_engine::ScanReport {
    let engine = Engine::with_builtin(&HashSet::new()).expect("内置规则编译失败");
    engine
        .scan(root, &CancelToken::new(), &|_| {})
        .expect("扫描失败")
}

#[test]
fn a1_determinism_byte_identical() {
    let dir = tempfile::tempdir().unwrap();
    build_corpus(dir.path());
    let first = scan_root(dir.path());
    let second = scan_root(dir.path());
    assert_eq!(
        serde_json::to_string(&first.findings).unwrap(),
        serde_json::to_string(&second.findings).unwrap()
    );
    assert_eq!(
        serde_json::to_string(&first.events).unwrap(),
        serde_json::to_string(&second.events).unwrap()
    );
    assert_eq!(first.stats, second.stats);
}

#[test]
fn a3_fail_closed_events() {
    let dir = tempfile::tempdir().unwrap();
    build_corpus(dir.path());
    let report = scan_root(dir.path());

    let has = |pred: &dyn Fn(&ScanEvent) -> bool| report.events.iter().filter(|e| pred(e)).count();
    assert!(
        has(&|e| matches!(e, ScanEvent::ParseError { path, .. } if path == "src/broken.ts")) >= 1,
        "缺解析失败事件：{:?}",
        report.events
    );
    assert!(has(&|e| matches!(e, ScanEvent::Binary { path } if path == "assets/blob.bin")) == 1);
    assert!(has(&|e| matches!(e, ScanEvent::Oversize { path, size, .. } if path == "src/huge.ts" && *size > 1024 * 1024)) == 1);
    assert!(
        has(&|e| matches!(e, ScanEvent::UnknownLanguageSummary { count } if *count == 1)) == 1,
        "事件清单：{:#?}",
        report.events
    ); // 仅 readme.md；.gitignore 属隐藏文件计入 Ignored
    // 被忽略文件计入汇总且不出现在发现中。
    assert!(has(&|e| matches!(e, ScanEvent::IgnoredSummary { count } if *count >= 1)) == 1);
    assert!(report.findings.iter().all(|f| f.path != "ignored-dir/skip.ts"));
    // 语法错误文件的命中必须带 degraded。
    for f in &report.findings {
        if f.path == "src/broken.ts" {
            assert!(f.degraded, "broken.ts 命中未标 degraded：{f:?}");
        }
    }
    // 二进制/超限/未知扩展不产发现。
    assert!(report
        .findings
        .iter()
        .all(|f| f.path != "assets/blob.bin" && f.path != "src/huge.ts" && f.path != "docs/readme.md"));
}

#[test]
fn a4_line_drift_stable_and_gate() {
    let dir = tempfile::tempdir().unwrap();
    // 一个会命中 TODO 文本规则的内容 + 上方插入空行制造行漂移。
    let v1 = dir.path().join("v1");
    std::fs::create_dir_all(&v1).unwrap();
    std::fs::write(v1.join("a.ts"), "const x = 1;\n// TODO: keep\n").unwrap();
    let report1 = scan_root(&v1);
    assert_eq!(report1.findings.len(), 1, "TODO 规则应命中一次");

    let v2 = dir.path().join("v2");
    std::fs::create_dir_all(&v2).unwrap();
    std::fs::write(v2.join("a.ts"), "\n\nconst x = 1;\n// TODO: keep\n").unwrap();
    let report2 = scan_root(&v2);
    assert_eq!(report1.findings[0].fingerprint, report2.findings[0].fingerprint);

    // 基线：修复后门禁为空；基线条目移除后恢复。
    let baseline = baseline::Baseline::snapshot(&report1);
    let fixed = dir.path().join("v3");
    std::fs::create_dir_all(&fixed).unwrap();
    std::fs::write(fixed.join("a.ts"), "const x = 1;\n").unwrap();
    let report_fixed = scan_root(&fixed);
    assert!(baseline.gate_new(&report_fixed).is_empty());

    let mut pruned = baseline;
    pruned.entries.clear();
    let report_again = scan_root(&v2);
    assert_eq!(pruned.gate_new(&report_again).len(), 1);
}

#[test]
fn a5_sarif_one_to_one() {
    let dir = tempfile::tempdir().unwrap();
    build_corpus(dir.path());
    let report = scan_root(dir.path());
    let value = sarif::export(&report);

    assert_eq!(value["version"], "2.1.0");
    let results = value["runs"][0]["results"].as_array().unwrap();
    assert_eq!(results.len(), report.findings.len(), "SARIF 与内部发现一一对应");
    let rules = value["runs"][0]["tool"]["driver"]["rules"].as_array().unwrap();
    let sarif_rule_ids: HashSet<&str> = rules
        .iter()
        .map(|r| r["id"].as_str().unwrap())
        .collect();
    for finding in &report.findings {
        assert!(sarif_rule_ids.contains(finding.rule_id.as_str()));
    }
    for (result, finding) in results.iter().zip(report.findings.iter()) {
        assert_eq!(result["ruleId"].as_str().unwrap(), finding.rule_id);
        assert_eq!(
            result["level"].as_str().unwrap(),
            finding.severity.to_sarif_level()
        );
        assert_eq!(
            result["locations"][0]["physicalLocation"]["artifactLocation"]["uri"]
                .as_str()
                .unwrap(),
            finding.path
        );
        let region = &result["locations"][0]["physicalLocation"]["region"];
        assert_eq!(region["startLine"].as_u64().unwrap(), finding.span.start.line as u64);
        assert_eq!(region["endLine"].as_u64().unwrap(), finding.span.end.line as u64);
        let degraded = result["properties"]["degraded"].as_bool().unwrap();
        assert_eq!(degraded, finding.degraded);
    }
    // degraded 断言真实覆盖：corpus 含 broken.ts。
    assert!(report.findings.iter().any(|f| f.degraded) || report.findings.is_empty());
}

/// A7：8 个语言枚举值全部走通「解析 + AST 规则命中」管线。
/// 使用临时自定义规则，避免依赖内置规则集内容。
#[test]
fn a7_language_matrix() {
    struct Case {
        file: &'static str,
        language: &'static str,
        pattern: &'static str,
        snippet: &'static str,
    }
    let cases = vec![
        Case { file: "a.ts", language: "typescript", pattern: "(return_statement) @match", snippet: "function f(): number {\n  return 1;\n}\n" },
        Case { file: "a.tsx", language: "tsx", pattern: "(return_statement) @match", snippet: "function f(): number {\n  return 1;\n}\n" },
        Case { file: "a.js", language: "javascript", pattern: "(return_statement) @match", snippet: "function f() {\n  return 1;\n}\n" },
        Case { file: "a.jsx", language: "jsx", pattern: "(return_statement) @match", snippet: "function f() {\n  return 1;\n}\n" },
        Case { file: "a.py", language: "python", pattern: "(return_statement) @match", snippet: "def f():\n    return 1\n" },
        Case { file: "A.java", language: "java", pattern: "(return_statement) @match", snippet: "class A {\n  int f() {\n    return 1;\n  }\n}\n" },
        Case { file: "a.go", language: "go", pattern: "(return_statement) @match", snippet: "package main\n\nfunc f() int {\n\treturn 1\n}\n" },
        Case { file: "a.rs", language: "rust", pattern: "(return_expression) @match", snippet: "fn f() -> i32 {\n    return 1;\n}\n" },
    ];

    for case in cases {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(case.file), case.snippet).unwrap();
        let custom = format!(
            "rules:\n  - id: probe/return\n    language: {}\n    dimension: quality\n    severity: info\n    message: probe\n    pattern: '{}'\n",
            case.language, case.pattern
        );
        let builtin_yaml = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/rules/builtin.yaml")).unwrap();
        let engine = Engine::build(&builtin_yaml, Some(&custom), &HashSet::new())
            .unwrap_or_else(|e| panic!("{} 规则编译失败：{:?}", case.language, e));
        let report = engine
            .scan(dir.path(), &CancelToken::new(), &|_| {})
            .unwrap_or_else(|e| panic!("{} 扫描失败：{e}", case.language));
        assert!(
            report
                .findings
                .iter()
                .any(|f| f.rule_id == "probe/return"),
            "{} 未命中探针规则：{:?}",
            case.language,
            report.findings
        );
    }
}

#[test]
fn a9_performance_1000_files() {
    let dir = tempfile::tempdir().unwrap();
    for i in 0..1000 {
        let content = format!(
            "export function f{i}(v: number): number {{\n  if (v > {i}) {{\n    return v + {i};\n  }}\n  return v;\n}}\n"
        );
        std::fs::write(dir.path().join(format!("m{i:04}.ts")), content).unwrap();
    }
    let engine = Engine::with_builtin(&HashSet::new()).unwrap();
    let start = std::time::Instant::now();
    let report = engine
        .scan(dir.path(), &CancelToken::new(), &|_| {})
        .expect("扫描失败");
    let elapsed = start.elapsed();
    assert_eq!(report.stats.files_scanned, 1000);
    assert!(
        elapsed.as_secs() < 90,
        "1000 文件扫描耗时 {:?} 超过 90s 硬上限",
        elapsed
    );
}

#[test]
fn cancelled_scan_returns_error_and_no_partial_report() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("a.ts"), "const x = 1;\n").unwrap();
    let engine = Engine::with_builtin(&HashSet::new()).unwrap();
    let cancel = CancelToken::new();
    cancel.cancel(); // 预先取消
    let result = engine.scan(dir.path(), &cancel, &|_| {});
    assert!(matches!(result, Err(codescan_engine::ScanError::Cancelled)));
    let _ = AtomicBool::new(false).compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst);
}

// 防未使用告警（PathBuf 供扩展用）。
#[allow(dead_code)]
fn _unused(_p: PathBuf) {}

/// B4：污点发现携带 source 位置，SARIF properties.flows 一一对应。
#[test]
fn b4_sarif_flows() {
    let dir = tempfile::tempdir().unwrap();
    write_file(
        dir.path(),
        "s.ts",
        "function run(userInput: string): void {\n  eval(userInput);\n}\n",
    );
    let report = scan_root(dir.path());
    let taint: Vec<_> = report
        .findings
        .iter()
        .filter(|f| f.rule_id == "ts/taint-eval-param")
        .collect();
    assert_eq!(taint.len(), 1, "污点规则应恰好命中一次：{:?}", report.findings);
    assert_eq!(taint[0].span.start.line, 2, "发现 span 应落在 sink 行");
    assert_eq!(taint[0].flows.len(), 1, "应携带一个 source 位置");
    assert_eq!(taint[0].flows[0].line, 1, "source 为函数参数行");

    let value = sarif::export(&report);
    let results = value["runs"][0]["results"].as_array().unwrap();
    let sarif_taint = results
        .iter()
        .find(|r| r["ruleId"] == "ts/taint-eval-param")
        .expect("SARIF 应包含污点结果");
    let flows = sarif_taint["properties"]["flows"].as_array().unwrap();
    assert_eq!(flows.len(), 1);
    assert_eq!(flows[0]["startLine"].as_u64().unwrap(), 1);
}

/// 任务 6：忽略源 = 内置默认目录 + `.codescanignore` + 全局忽略文件，且全部计入汇总事件。
#[test]
fn ignore_sources_and_defaults() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let write = |rel: &str, content: &str| {
        let path = root.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, content).unwrap();
    };
    // 会被内置默认目录排除。
    write("node_modules/dep/index.ts", "// TODO: vendor noise\n");
    // 会被 .codescanignore 排除。
    write(".codescanignore", "generated/\n");
    write("generated/out.ts", "// TODO: generated\n");
    // 会被全局忽略文件排除。
    let global_ignore = dir.path().join("global-ignore");
    std::fs::write(&global_ignore, "local-only/\n").unwrap();
    write("local-only/scratch.ts", "// TODO: local scratch\n");
    // 正常扫描。
    write("src/app.ts", "// TODO: real debt\n");

    let engine = Engine::with_builtin(&HashSet::new()).unwrap();
    let options = codescan_engine::scan::ScanOptions {
        global_ignore_file: Some(global_ignore.clone()),
    };
    let report = engine
        .scan_with_options(root, &options, &CancelToken::new(), &|_| {})
        .expect("扫描失败");

    let todo: Vec<&str> = report
        .findings
        .iter()
        .filter(|f| f.rule_id == "text/todo-debt")
        .map(|f| f.path.as_str())
        .collect();
    assert_eq!(todo, vec!["src/app.ts"], "仅真实源码应被扫描：{todo:?}");
    assert!(
        report
            .events
            .iter()
            .any(|e| matches!(e, ScanEvent::IgnoredSummary { count } if *count >= 3)),
        "被忽略文件应计入汇总事件：{:?}",
        report.events
    );
    // 无全局忽略文件时 local-only 会被扫描（证明选项生效）。
    let report_plain = engine
        .scan(root, &CancelToken::new(), &|_| {})
        .expect("扫描失败");
    assert!(report_plain
        .findings
        .iter()
        .any(|f| f.path == "local-only/scratch.ts"));
}

/// D5：跨文件流不追踪（同文件过程间边界的负例）。
#[test]
fn d5_cross_file_flow_not_tracked() {
    let dir = tempfile::tempdir().unwrap();
    write_file(
        dir.path(),
        "a.ts",
        "function entry(): void {\n  const v = readInput();\n  runEval(v);\n}\n",
    );
    write_file(
        dir.path(),
        "b.ts",
        "function runEval(code: string): void {\n  eval(code);\n}\n",
    );
    let custom = "rules:\n  - id: custom/xfile\n    language: typescript\n    dimension: security\n    severity: error\n    message: m\n    taint:\n      sources:\n        - |\n          (call_expression\n            function: (identifier) @fn (#eq? @fn \"readInput\")) @match\n      sinks:\n        - |\n          (call_expression\n            function: (identifier) @fn (#eq? @fn \"eval\")) @match\n";
    let builtin_yaml =
        std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/rules/builtin.yaml")).unwrap();
    let engine = Engine::build(&builtin_yaml, Some(custom), &HashSet::new()).unwrap();
    let report = engine
        .scan(dir.path(), &CancelToken::new(), &|_| {})
        .expect("扫描失败");
    let cross: Vec<_> = report
        .findings
        .iter()
        .filter(|f| f.rule_id == "custom/xfile")
        .collect();
    assert!(
        cross.is_empty(),
        "跨文件流不应被追踪：{cross:?}"
    );
}
