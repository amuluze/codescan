//! v3.3 候选规则隔离回归（Task Spec G1–G5）。
//!
//! 在种子发现所在文件的临时副本上，按规则套件语义（标注行 ↔ 实际命中）
//! 仅启用候选规则重扫；全程只读扫描历史/基线，不写 findings。
//! 临时目录使用 `std::env::temp_dir()`，无需新增依赖。

use crate::llm;
use crate::store::Store;
use codescan_engine::language::Language;
use codescan_engine::rules::{parse_rule_set, RuleLanguage, RuleSpec};
use codescan_engine::{CancelToken, Engine};
use serde::Serialize;
use std::collections::{BTreeMap, HashSet};
use std::path::{Component, Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// 实际命中行（多重集按行展开）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RegressionActual {
    pub line: usize,
    pub message: String,
}

/// 隔离回归结果：仅展示，不参与门禁/基线。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CandidateRegressionResult {
    /// 候选规则本身是否可编译（非法候选不进入扫描）。
    pub valid: bool,
    /// 期望/实际 `(规则, 行)` 多重集是否完全一致。
    pub pass: bool,
    /// 候选校验错误（`valid=false` 时非空）。
    pub errors: Vec<String>,
    /// 期望命中行（按规则套件「标注指向下一非空行」语义得出）。
    pub expected: Vec<usize>,
    /// 隔离扫描的实际命中行与消息。
    pub actual: Vec<RegressionActual>,
    /// 漏报 / 过度匹配的可读说明。
    pub failures: Vec<String>,
    /// `save_fixture=true` 时写出的夹具路径。
    pub saved_paths: Vec<String>,
}

impl CandidateRegressionResult {
    fn invalid(errors: Vec<String>) -> Self {
        Self {
            valid: false,
            pass: false,
            errors,
            expected: Vec::new(),
            actual: Vec::new(),
            failures: Vec::new(),
            saved_paths: Vec::new(),
        }
    }
}

fn rule_errors(errs: &[codescan_engine::rules::RuleError]) -> String {
    errs.iter()
        .map(|e| e.to_string())
        .collect::<Vec<_>>()
        .join("; ")
}

/// 从 Store 取扫描 root + 发现，执行隔离回归。
///
/// 候选非法时**先于**任何存储/文件访问返回结构化结果；源文件缺失等
/// 环境问题返回可读 `Err`，不 panic。
pub fn run_from_store(
    store: &Store,
    data_root: &Path,
    scan_id: i64,
    fingerprint: &str,
    candidate_yaml: &str,
    save_fixture: bool,
) -> Result<CandidateRegressionResult, String> {
    let errors = llm::validate_candidate(candidate_yaml);
    if !errors.is_empty() {
        return Ok(CandidateRegressionResult::invalid(errors));
    }

    let root = store.get_scan_root(scan_id).map_err(|e| e.to_string())?;
    let findings = store.get_findings(scan_id).map_err(|e| e.to_string())?;
    let finding = findings
        .iter()
        .find(|f| f.fingerprint == fingerprint)
        .ok_or_else(|| "未找到该发现".to_string())?;

    let fixture_base = save_fixture.then(|| data_root.join("rules").join("tests"));
    regress_file(
        Path::new(&root),
        &finding.path,
        finding.span.start.line,
        candidate_yaml,
        fixture_base.as_deref(),
    )
}

/// 在 `files_root/relative_path` 的临时副本上执行一次隔离回归。
fn regress_file(
    files_root: &Path,
    relative_path: &str,
    start_line: usize,
    candidate_yaml: &str,
    fixture_base: Option<&Path>,
) -> Result<CandidateRegressionResult, String> {
    let errors = llm::validate_candidate(candidate_yaml);
    if !errors.is_empty() {
        return Ok(CandidateRegressionResult::invalid(errors));
    }

    let rel = Path::new(relative_path);
    if relative_path.is_empty() {
        return Err("发现路径为空，无法读取源文件".to_string());
    }
    let mut source_path = files_root.to_path_buf();
    for component in rel.components() {
        match component {
            Component::Normal(part) => source_path.push(part),
            _ => return Err(format!("发现路径不合法：{relative_path}")),
        }
    }
    let file_name = rel
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| format!("无法识别文件名：{relative_path}"))?;
    let language = Language::detect(Path::new(file_name))
        .ok_or_else(|| format!("不支持的语言扩展名：{file_name}"))?;
    let marker = if language == Language::Python { "#" } else { "//" };

    let source = std::fs::read_to_string(&source_path)
        .map_err(|e| format!("读取源文件失败：{}（{e}）", source_path.display()))?;
    let line_count = source.lines().count();
    if start_line == 0 || start_line > line_count {
        return Err(format!(
            "发现起始行 {start_line} 超出源文件行数 {line_count}：{relative_path}"
        ));
    }

    let specs = parse_rule_set(candidate_yaml)
        .map_err(|errs| rule_errors(&errs))?;
    let applicable: Vec<String> = specs
        .iter()
        .filter(|spec| rule_applies_to(spec, language))
        .map(|spec| spec.id.clone())
        .collect();
    if applicable.is_empty() {
        return Ok(CandidateRegressionResult {
            valid: true,
            pass: false,
            errors: Vec::new(),
            expected: Vec::new(),
            actual: Vec::new(),
            failures: vec![format!(
                "候选规则不适用于该文件语言（{}）",
                language.rule_value()
            )],
            saved_paths: Vec::new(),
        });
    }

    let annotated = annotate(&source, start_line, marker, &applicable);
    let expected_pairs: Vec<(String, usize)> = parse_annotations(&annotated, marker)
        .into_iter()
        .filter(|(id, _)| applicable.contains(id))
        .collect();

    let workspace =
        TempWorkspace::new().map_err(|e| format!("创建临时目录失败：{e}"))?;
    let fixture_path = workspace.path().join(file_name);
    std::fs::write(&fixture_path, &annotated)
        .map_err(|e| format!("写入临时语料失败：{}（{e}）", fixture_path.display()))?;

    // 禁用全部内置规则：只让候选规则参与扫描。
    let builtin = Engine::with_builtin(&HashSet::new())
        .map_err(|errs| format!("内置规则编译失败：{}", rule_errors(&errs)))?;
    let disabled: HashSet<String> = builtin.all_rule_ids().into_iter().collect();
    let engine = Engine::with_rules(Some(candidate_yaml), &disabled)
        .map_err(|errs| format!("候选规则编译失败：{}", rule_errors(&errs)))?;
    let report = engine
        .scan(workspace.path(), &CancelToken::new(), &|_| {})
        .map_err(|e| format!("隔离扫描失败：{e}"))?;

    let actual_pairs: Vec<(String, usize)> = report
        .findings
        .iter()
        .map(|f| (f.rule_id.clone(), f.span.start.line))
        .collect();
    let failures = diff(&expected_pairs, &actual_pairs);

    let mut expected: Vec<usize> = expected_pairs.iter().map(|(_, line)| *line).collect();
    expected.sort_unstable();
    let mut actual: Vec<RegressionActual> = report
        .findings
        .iter()
        .map(|f| RegressionActual {
            line: f.span.start.line,
            message: f.message.clone(),
        })
        .collect();
    actual.sort_by(|a, b| a.line.cmp(&b.line).then_with(|| a.message.cmp(&b.message)));

    let mut saved_paths = Vec::new();
    if let Some(base) = fixture_base {
        if let Some(rule_id) = expected_pairs
            .first()
            .map(|(id, _)| id)
            .or_else(|| applicable.first())
        {
            let dir = base.join(safe_component(rule_id));
            std::fs::create_dir_all(&dir)
                .map_err(|e| format!("创建夹具目录失败：{}（{e}）", dir.display()))?;
            let saved = dir.join(file_name);
            std::fs::write(&saved, &annotated)
                .map_err(|e| format!("写入夹具失败：{}（{e}）", saved.display()))?;
            saved_paths.push(saved.to_string_lossy().into_owned());
        }
    }

    Ok(CandidateRegressionResult {
        valid: true,
        pass: failures.is_empty(),
        errors: Vec::new(),
        expected,
        actual,
        failures,
        saved_paths,
    })
}

fn rule_applies_to(spec: &RuleSpec, language: Language) -> bool {
    match spec.language {
        RuleLanguage::All => true,
        RuleLanguage::One(spec_language) => spec_language == language,
    }
}

/// 在发现起始行（1-based）上方插入一条/多条 `ruleid:` 标注；保留原换行风格。
fn annotate(source: &str, start_line: usize, marker: &str, rule_ids: &[String]) -> String {
    let eol = if source.contains("\r\n") { "\r\n" } else { "\n" };
    let insert_index = start_line - 1;
    let mut out = String::with_capacity(source.len() + rule_ids.len() * (marker.len() + 64));
    for (idx, line) in source.split_inclusive('\n').enumerate() {
        if idx == insert_index {
            for id in rule_ids {
                out.push_str(marker);
                out.push_str(" ruleid: ");
                out.push_str(id);
                out.push_str(eol);
            }
        }
        out.push_str(line);
    }
    out
}

/// 规则套件语义：连续标注指向其后第一个非空且非标注行。
fn parse_annotations(source: &str, marker: &str) -> Vec<(String, usize)> {
    let mut expected = Vec::new();
    let mut pending: Vec<String> = Vec::new();
    for (idx, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();
        let after_marker = trimmed
            .strip_prefix(marker)
            .map(|rest| rest.trim_start());
        if let Some(rest) = after_marker {
            if let Some(rule_id) = rest.strip_prefix("ruleid:") {
                let rule_id = rule_id.trim();
                if !rule_id.is_empty() {
                    pending.push(rule_id.to_string());
                }
                continue;
            }
        }
        if trimmed.is_empty() {
            continue;
        }
        for rule_id in pending.drain(..) {
            expected.push((rule_id, idx + 1));
        }
    }
    expected
}

/// 多重集比较 `(规则, 行)`，输出漏报/过度匹配说明。
fn diff(expected: &[(String, usize)], actual: &[(String, usize)]) -> Vec<String> {
    let expected_counts = counter(expected);
    let actual_counts = counter(actual);
    let mut remaining = expected_counts;
    let mut failures = Vec::new();
    for (pair, count) in &actual_counts {
        let expected_count = remaining.get(pair).copied().unwrap_or(0);
        if *count > expected_count {
            for _ in 0..(*count - expected_count) {
                failures.push(format!(
                    "过度匹配：规则 {} 在第 {} 行命中但未标注",
                    pair.0, pair.1
                ));
            }
            remaining.remove(pair);
        } else if *count < expected_count {
            remaining.insert((*pair).clone(), expected_count - *count);
        } else {
            remaining.remove(pair);
        }
    }
    for (pair, count) in remaining {
        for _ in 0..count {
            failures.push(format!(
                "漏报：规则 {} 未命中期望行 {}",
                pair.0, pair.1
            ));
        }
    }
    failures.sort();
    failures
}

fn counter(pairs: &[(String, usize)]) -> BTreeMap<(String, usize), usize> {
    let mut counts = BTreeMap::new();
    for pair in pairs {
        *counts.entry((*pair).clone()).or_insert(0) += 1;
    }
    counts
}

/// 规则 id 作为目录名：剥除路径分隔等不安全字符，避免逃逸数据根。
fn safe_component(rule_id: &str) -> String {
    let sanitized: String = rule_id
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.') {
                c
            } else {
                '_'
            }
        })
        .collect();
    if sanitized.is_empty() || sanitized == "." || sanitized == ".." {
        "rule".to_string()
    } else {
        sanitized
    }
}

struct TempWorkspace {
    path: PathBuf,
}

impl TempWorkspace {
    fn new() -> std::io::Result<Self> {
        let base = std::env::temp_dir();
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0);
        for attempt in 0..100u32 {
            let path = base.join(format!(
                "codescan-regression-{}-{nanos}-{attempt}",
                std::process::id()
            ));
            match std::fs::create_dir(&path) {
                Ok(()) => return Ok(Self { path }),
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(e) => return Err(e),
            }
        }
        Err(std::io::Error::new(
            std::io::ErrorKind::AlreadyExists,
            "无法创建唯一临时目录",
        ))
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempWorkspace {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}
