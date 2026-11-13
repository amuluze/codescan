//! A2 规则套件：每个 fixture 目录运行全部内置规则，期望集合与实际命中
//! 集合精确相等（Task Spec「规则 fixture 格式」）。

use codescan_engine::{CancelToken, Engine};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

const RULES_TESTS_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/rules");

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
    let mut pending: Vec<String> = Vec::new();
    for (idx, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();
        let after_marker = trimmed
            .strip_prefix(marker)
            .map(|rest| rest.trim_start());
        if let Some(rest) = after_marker {
            if let Some(rule_id) = rest.strip_prefix("ruleid:") {
                let rule_id = rule_id.trim().to_string();
                if !rule_id.is_empty() {
                    pending.push(rule_id);
                }
                continue; // 标注行本身不是目标行
            }
        }
        if trimmed.is_empty() {
            continue; // 空行不消费待定标注
        }
        // 连续标注共同指向其后第一个非空且非标注行。
        for rule_id in pending.drain(..) {
            expected.push((rule_id, file_name.to_string(), idx + 1));
        }
    }
    expected
}

fn fixture_dirs(dir_prefix: Option<&str>) -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = std::fs::read_dir(RULES_TESTS_DIR)
        .expect("tests/fixtures/rules 目录存在")
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

        // 多重集比较：同 (rule, file, line) 出现 N 次即要求 N 次。
        let expected_counts: BTreeMap<Expectation, usize> = expected
            .iter()
            .fold(BTreeMap::new(), |mut acc, key| {
                *acc.entry(key.clone()).or_insert(0) += 1;
                acc
            });
        let actual_counts: BTreeMap<Expectation, usize> = report
            .findings
            .iter()
            .fold(BTreeMap::new(), |mut acc, f| {
                *acc.entry((f.rule_id.clone(), f.path.clone(), f.span.start.line))
                    .or_insert(0) += 1;
                acc
            });

        if expected_counts != actual_counts {
            let mut missing: Vec<String> = Vec::new();
            let mut extra: Vec<String> = Vec::new();
            for (key, want) in &expected_counts {
                let got = actual_counts.get(key).copied().unwrap_or(0);
                if got < *want {
                    missing.push(format!(
                        "漏报 {rule} @ {file}:{line}（期望 {want} 次，实际 {got} 次）",
                        rule = key.0,
                        file = key.1,
                        line = key.2
                    ));
                }
            }
            for (key, got) in &actual_counts {
                let want = expected_counts.get(key).copied().unwrap_or(0);
                if *got > want {
                    extra.push(format!(
                        "误报 {rule} @ {file}:{line}（期望 {want} 次，实际 {got} 次）",
                        rule = key.0,
                        file = key.1,
                        line = key.2
                    ));
                }
            }
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
