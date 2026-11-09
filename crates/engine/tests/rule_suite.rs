//! A2 规则套件：每个 fixture 目录运行全部内置规则，期望集合与实际命中
//! 集合精确相等（Task Spec「规则 fixture 格式」）。

use codescan_engine::{CancelToken, Engine};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

const RULES_TESTS_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/rules/tests");

/// (rule_id, file, line)
type Expectation = (String, String, usize);

fn comment_marker(extension: &str) -> &'static str {
    match extension {
        "py" => "#",
        _ => "//",
    }
}

fn parse_annotations(path: &Path, file_name: &str, source: &str) -> Vec<Expectation> {
    let extension = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or_default();
    let marker = comment_marker(extension);
    let mut expected = Vec::new();
    for (idx, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();
        let after_marker = trimmed
            .strip_prefix(marker)
            .map(|rest| rest.trim_start());
        if let Some(rest) = after_marker {
            if let Some(rule_id) = rest.strip_prefix("ruleid:") {
                let rule_id = rule_id.trim().to_string();
                if !rule_id.is_empty() {
                    expected.push((rule_id, file_name.to_string(), idx + 2)); // 标注的下一行
                }
            }
        }
    }
    expected
}

fn fixture_dirs(dir_prefix: Option<&str>) -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = std::fs::read_dir(RULES_TESTS_DIR)
        .expect("rules/tests 目录存在")
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .filter(|p| {
            dir_prefix.is_none_or(|prefix| {
                p.file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|name| name.starts_with(prefix))
            })
        })
        .collect();
    dirs.sort();
    dirs
}

#[test]
fn rule_fixtures_match_exactly() {
    let engine = Engine::with_builtin(&Default::default()).expect("内置规则编译失败");
    let cancel = CancelToken::new();
    // 并行分工用：只检查指定前缀的 fixture 目录与规则 id（未设置则全量）。
    let dir_prefix = std::env::var("CODESCAN_FIXTURE_DIR").ok();
    let rule_prefixes: Vec<String> = std::env::var("CODESCAN_RULE_PREFIX")
        .map(|v| v.split(',').map(|s| s.trim().to_string()).collect())
        .unwrap_or_default();
    let mut failures: Vec<String> = Vec::new();
    let mut covered_rule_ids: BTreeSet<String> = BTreeSet::new();

    for dir in fixture_dirs(dir_prefix.as_deref()) {
        let report = engine
            .scan(&dir, &cancel, &|_| {})
            .expect("fixture 目录扫描失败");

        let mut expected: BTreeSet<Expectation> = BTreeSet::new();
        for entry in std::fs::read_dir(&dir).expect("读 fixture 目录") .flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or_default()
                .to_string();
            if let Ok(source) = std::fs::read_to_string(&path) {
                expected.extend(parse_annotations(&path, &name, &source));
            }
        }
        for (rule_id, _, _) in &expected {
            covered_rule_ids.insert(rule_id.clone());
        }

        let actual: BTreeSet<Expectation> = report
            .findings
            .iter()
            .map(|f| {
                (
                    f.rule_id.clone(),
                    f.path.clone(),
                    f.span.start.line,
                )
            })
            .collect();

        if expected != actual {
            let missing: Vec<String> = expected
                .difference(&actual)
                .map(|(r, f, l)| format!("漏报 {r} @ {f}:{l}"))
                .collect();
            let extra: Vec<String> = actual
                .difference(&expected)
                .map(|(r, f, l)| format!("误报 {r} @ {f}:{l}"))
                .collect();
            failures.push(format!(
                "fixture 目录 {:?}：\n  {}\n  {}",
                dir.file_name().unwrap_or_default(),
                missing.join("; "),
                extra.join("; ")
            ));
        }
    }

    // A2 覆盖性：每条内置规则至少被一个 fixture 标注覆盖。
    let all_rule_ids: Vec<String> = engine
        .all_rule_ids()
        .into_iter()
        .filter(|id| {
            rule_prefixes.is_empty()
                || rule_prefixes.iter().any(|p| id.starts_with(p.as_str()))
        })
        .collect();
    let uncovered: Vec<&String> = all_rule_ids
        .iter()
        .filter(|id| !covered_rule_ids.contains(*id))
        .collect();
    if !uncovered.is_empty() {
        failures.push(format!("未被 fixture 覆盖的规则：{uncovered:?}"));
    }

    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
