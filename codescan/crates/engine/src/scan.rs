//! 扫描编排：walk → classify → parse → 匹配 → 度量 → 指纹/排序。
//! 确定性（Domain §2）：文件列表排序后处理，rayon 仅做无共享并行，
//! 汇总后统一排序；事件与发现均不携带机器差异。

use crate::compile::{AstRule, CompiledRule, MetricKind, TextRule};
use crate::fingerprint;
use crate::language::{self, Language};
use crate::metrics;
use crate::model::{
    CancelToken, Finding, Progress, ScanError, ScanEvent, ScanReport, ScanStats, Span, Position,
};
use crate::rules::{self, RuleError, RuleLanguage, RuleSpec};
use rayon::prelude::*;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// 单文件大小上限（Task Spec 冻结的引擎常量）。
pub const MAX_FILE_BYTES: u64 = 1024 * 1024;

/// 内置默认排除目录（与语言无关的产物/依赖目录；`.codescanignore` 无法反选）。
pub const DEFAULT_IGNORED_DIRS: &[&str] = &[
    "node_modules",
    "target",
    "dist",
    "build",
    ".venv",
    "__pycache__",
    ".next",
    "coverage",
];

/// 扫描选项（Domain §3.1：忽略源 = .gitignore + .codescanignore + 全局忽略文件 + 内置默认）。
#[derive(Debug, Clone, Default)]
pub struct ScanOptions {
    /// 额外全局忽略文件（gitignore 语法，应用于工作区内任意路径）。
    pub global_ignore_file: Option<std::path::PathBuf>,
}

pub struct Engine {
    ast_rules: Vec<AstRule>,
    text_rules: Vec<TextRule>,
    metric_configs: Vec<(MetricKind, Vec<(String, f64)>)>,
    /// 度量规则的原始声明（UI 规则列表展示用）。
    pub metric_specs: Vec<RuleSpec>,
    taint_rules: Vec<crate::compile::TaintRule>,
    pub engine_version: String,
}

impl Engine {
    /// 内置规则构建（无自定义规则）。
    pub fn with_builtin(disabled: &HashSet<String>) -> Result<Engine, Vec<RuleError>> {
        Self::with_rules(None, disabled)
    }

    /// 内置 + 可选自定义规则构建；id 冲突或启用集编译错误整体拒绝。
    pub fn with_rules(
        custom_yaml: Option<&str>,
        disabled: &HashSet<String>,
    ) -> Result<Engine, Vec<RuleError>> {
        const BUILTIN_RULE_FILES: &[&str] = &[
            include_str!("../rules/builtin.yaml"),
            include_str!("../rules/typescript.yaml"),
            include_str!("../rules/python.yaml"),
            include_str!("../rules/java.yaml"),
            include_str!("../rules/go.yaml"),
            include_str!("../rules/rust.yaml"),
        ];
        let mut builtin = Vec::new();
        let mut errors = Vec::new();
        for file in BUILTIN_RULE_FILES {
            match rules::parse_rule_set(file) {
                Ok(mut specs) => builtin.append(&mut specs),
                Err(mut errs) => errors.append(&mut errs),
            }
        }
        if !errors.is_empty() {
            return Err(errors);
        }
        Self::build_from_specs(builtin, custom_yaml, disabled)
    }

    /// 内置 + 自定义规则构建（单文件 YAML 入口）；id 冲突或启用集编译错误整体拒绝。
    pub fn build(
        builtin_yaml: &str,
        custom_yaml: Option<&str>,
        disabled: &HashSet<String>,
    ) -> Result<Engine, Vec<RuleError>> {
        let builtin = rules::parse_rule_set(builtin_yaml)?;
        Self::build_from_specs(builtin, custom_yaml, disabled)
    }

    fn build_from_specs(
        builtin: Vec<RuleSpec>,
        custom_yaml: Option<&str>,
        disabled: &HashSet<String>,
    ) -> Result<Engine, Vec<RuleError>> {
        let custom = match custom_yaml {
            Some(yaml) => rules::parse_rule_set(yaml)?,
            None => Vec::new(),
        };
        let merged = rules::merge_rule_sets(builtin, custom)?;
        // 校验范围 = 启用集（Domain §3.3）。
        let enabled: Vec<RuleSpec> = merged
            .into_iter()
            .filter(|spec| !disabled.contains(&spec.id))
            .collect();
        let compiled = crate::compile::compile(enabled)?;

        let mut ast_rules = Vec::new();
        let mut text_rules = Vec::new();
        let mut metric_configs = Vec::new();
        let mut metric_specs = Vec::new();
        let mut taint_rules = Vec::new();
        for rule in compiled {
            match rule {
                CompiledRule::Ast(r) => ast_rules.push(r),
                CompiledRule::Text(r) => text_rules.push(r),
                CompiledRule::Metric(r) => {
                    metric_configs.push((r.kind, r.options.clone()));
                    metric_specs.push(r.spec);
                }
                CompiledRule::Taint(r) => taint_rules.push(r),
            }
        }

        Ok(Engine {
            ast_rules,
            text_rules,
            metric_configs,
            metric_specs,
            taint_rules,
            engine_version: env!("CARGO_PKG_VERSION").to_string(),
        })
    }

    /// 规则目录（UI 规则管理页）：(id, language, dimension, severity, message, source)。
    /// source 为规则体的可读重建（query / 正则 / 度量名）。
    pub fn rule_catalog(&self) -> Vec<(String, String, String, String, String, String)> {
        let mut out = Vec::new();
        for rule in &self.ast_rules {
            out.push(catalog_entry(
                &rule.spec,
                rule.spec.pattern.clone().unwrap_or_default(),
            ));
        }
        for rule in &self.text_rules {
            out.push(catalog_entry(
                &rule.spec,
                rule.spec.text.clone().unwrap_or_default(),
            ));
        }
        for spec in &self.metric_specs {
            out.push(catalog_entry(
                spec,
                format!(
                    "{} {}",
                    spec.metric.clone().unwrap_or_default(),
                    spec.options
                        .iter()
                        .map(|(k, v)| format!("{k}={v}"))
                        .collect::<Vec<_>>()
                        .join(" ")
                ),
            ));
        }
        for rule in &self.taint_rules {
            out.push(catalog_entry(
                &rule.spec,
                format!(
                    "污点：{} source → {} sink（sanitizer {}）",
                    rule.source_queries.len(),
                    rule.sink_queries.len(),
                    rule.sanitizer_queries.len()
                ),
            ));
        }
        out.sort_by(|a, b| a.0.cmp(&b.0));
        out
    }

    /// 全部内置规则 id（启停无关，用于覆盖性检查）。
    pub fn all_rule_ids(&self) -> Vec<String> {
        let mut ids: Vec<String> = self
            .ast_rules
            .iter()
            .map(|r| r.spec.id.clone())
            .chain(self.text_rules.iter().map(|r| r.spec.id.clone()))
            .chain(self.metric_specs.iter().map(|s| s.id.clone()))
            .chain(self.taint_rules.iter().map(|r| r.spec.id.clone()))
            .collect();
        ids.sort();
        ids
    }

    pub fn scan(
        &self,
        root: &Path,
        cancel: &CancelToken,
        progress: &(dyn Fn(Progress) + Sync),
    ) -> Result<ScanReport, ScanError> {
        self.scan_with_options(root, &ScanOptions::default(), cancel, progress)
    }

    pub fn scan_with_options(
        &self,
        root: &Path,
        options: &ScanOptions,
        cancel: &CancelToken,
        progress: &(dyn Fn(Progress) + Sync),
    ) -> Result<ScanReport, ScanError> {
        progress(Progress::Walking);

        // 1. 收集文件（排序保证确定性处理顺序）。
        let mut files: Vec<(PathBuf, String)> = Vec::new();
        for entry in walk_workspace(root, options) {
            let (abs, rel) = entry;
            files.push((abs, rel));
        }
        files.sort_by(|a, b| a.1.cmp(&b.1));

        let raw_total = count_all_files(root);
        let ignored_count = raw_total.saturating_sub(files.len() as u64);

        // 2. 并行处理。
        progress(Progress::Scanning {
            done: 0,
            total: files.len() as u64,
        });
        let done_counter = std::sync::atomic::AtomicU64::new(0);
        let unknown_counter = std::sync::atomic::AtomicU64::new(0);

        let results: Vec<FileOutcome> = files
            .par_iter()
            .map(|(abs, rel)| {
                let done = done_counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
                progress(Progress::Scanning {
                    done,
                    total: files.len() as u64,
                });
                if cancel.is_cancelled() {
                    return FileOutcome::Cancelled;
                }
                self.scan_file(abs, rel, &unknown_counter)
            })
            .collect();

        if cancel.is_cancelled() || results.iter().any(|r| matches!(r, FileOutcome::Cancelled)) {
            return Err(ScanError::Cancelled);
        }

        // 3. 汇总。
        progress(Progress::Finalizing);
        let mut findings: Vec<Finding> = Vec::new();
        let mut events: Vec<ScanEvent> = Vec::new();
        let mut corpus: Vec<(String, Vec<(String, usize)>)> = Vec::new();
        let mut taint_facts: std::collections::HashMap<
            String,
            Vec<crate::taint::FileFacts>,
        > = std::collections::HashMap::new();
        let mut scanned = 0u64;

        for (outcome, (_, rel)) in results.into_iter().zip(files.iter()) {
            match outcome {
                FileOutcome::Scanned {
                    mut file_findings,
                    mut file_events,
                    normalized_lines,
                    taint_facts: file_taint,
                } => {
                    scanned += 1;
                    if let Some(lines) = normalized_lines {
                        corpus.push((rel.clone(), lines));
                    }
                    findings.append(&mut file_findings);
                    events.append(&mut file_events);
                    for (rule_id, facts) in file_taint {
                        taint_facts.entry(rule_id).or_default().push(facts);
                    }
                }
                FileOutcome::EventOnly(mut file_events) => {
                    events.append(&mut file_events);
                }
                FileOutcome::Cancelled => unreachable!("取消已在上方短路"),
            }
        }

        // 语料级度量：重复块。
        findings.extend(metrics::duplicate_blocks(&corpus, &self.metric_configs));

        // 污点：项目级分析（显式导入跨文件 + 字段/对象组；文件顺序由 facts 收集顺序保证）。
        for rule in &self.taint_rules {
            if let Some(facts) = taint_facts.get(&rule.spec.id) {
                findings.extend(crate::taint::analyze_project(rule, facts));
            }
        }

        // 4. 指纹 + 全序 + 事件排序。
        let findings = fingerprint::finalize(findings);
        let unknown_count = unknown_counter.load(std::sync::atomic::Ordering::SeqCst);
        if unknown_count > 0 {
            events.push(ScanEvent::UnknownLanguageSummary {
                count: unknown_count,
            });
        }
        if ignored_count > 0 {
            events.push(ScanEvent::IgnoredSummary {
                count: ignored_count,
            });
        }
        ScanEvent::sort(&mut events);

        let stats = ScanStats {
            files_total: raw_total,
            files_scanned: scanned,
            files_ignored: ignored_count,
            files_unknown_language: unknown_count,
            findings: findings.len() as u64,
        };

        Ok(ScanReport {
            engine_version: self.engine_version.clone(),
            findings,
            events,
            stats,
        })
    }


    fn scan_file(
        &self,
        abs: &Path,
        rel: &str,
        unknown_counter: &std::sync::atomic::AtomicU64,
    ) -> FileOutcome {
        let metadata = match std::fs::metadata(abs) {
            Ok(m) => m,
            Err(e) => {
                return FileOutcome::EventOnly(vec![ScanEvent::IoError {
                    path: rel.to_string(),
                    message: e.to_string(),
                }])
            }
        };
        if metadata.len() > MAX_FILE_BYTES {
            return FileOutcome::EventOnly(vec![ScanEvent::Oversize {
                path: rel.to_string(),
                size: metadata.len(),
            }]);
        }
        // 读内容 + 二进制检测先于语言识别：二进制证据比「未知扩展名」更具体。
        let bytes = match std::fs::read(abs) {
            Ok(bytes) => bytes,
            Err(e) => {
                return FileOutcome::EventOnly(vec![ScanEvent::IoError {
                    path: rel.to_string(),
                    message: e.to_string(),
                }])
            }
        };
        if bytes.len() > MAX_FILE_BYTES as usize {
            return FileOutcome::EventOnly(vec![ScanEvent::Oversize {
                path: rel.to_string(),
                size: bytes.len() as u64,
            }]);
        }
        if bytes[..bytes.len().min(8192)].contains(&0) {
            return FileOutcome::EventOnly(vec![ScanEvent::Binary {
                path: rel.to_string(),
            }]);
        }
        let lang = match Language::detect(abs) {
            Some(lang) => lang,
            None => {
                unknown_counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                return FileOutcome::EventOnly(Vec::new());
            }
        };

        let parsed = language::parse(lang, &bytes);
        let degraded = parsed.error_nodes > 0;
        let mut events = Vec::new();
        if degraded {
            events.push(ScanEvent::ParseError {
                path: rel.to_string(),
                error_nodes: parsed.error_nodes,
            });
        }

        let source = &parsed.source;
        let tree = &parsed.tree;
        let mut findings = Vec::new();

        for rule in &self.ast_rules {
            if let RuleLanguage::One(l) = rule.spec.language {
                if l != lang {
                    continue;
                }
                findings.extend(run_ast_rule(rule, lang, tree, source, rel, degraded));
            }
        }
        for rule in &self.text_rules {
            findings.extend(run_text_rule(rule, source, rel, degraded));
        }
        let mut taint_facts: Vec<(String, crate::taint::FileFacts)> = Vec::new();
        for rule in &self.taint_rules {
            if let RuleLanguage::One(l) = rule.spec.language {
                if l != lang {
                    continue;
                }
                taint_facts.push((
                    rule.spec.id.clone(),
                    crate::taint::extract_file(rule, lang, tree, source, rel, degraded),
                ));
            }
        }
        findings.extend(metrics::file_metrics(
            lang,
            tree,
            source,
            rel,
            degraded,
            &self.metric_configs,
        ));

        // 重复块语料输入：非空、非纯注释的规范化行（注释不算重复代码）。
        let normalized_lines: Vec<(String, usize)> = source
            .lines()
            .enumerate()
            .map(|(idx, line)| (line.trim(), idx + 1))
            .filter(|(text, _)| {
                !text.is_empty() && !text.starts_with("//") && !text.starts_with('#')
            })
            .map(|(text, line)| (text.to_string(), line))
            .collect();

        FileOutcome::Scanned {
            file_findings: findings,
            file_events: events,
            normalized_lines: Some(normalized_lines),
            taint_facts,
        }
    }
}


fn catalog_entry(
    spec: &RuleSpec,
    source: String,
) -> (String, String, String, String, String, String) {
    let language = match spec.language {
        RuleLanguage::All => "*".into(),
        RuleLanguage::One(lang) => lang.rule_value().into(),
    };
    let dimension = serde_json::to_string(&spec.dimension)
        .unwrap_or_default()
        .trim_matches('"')
        .to_string();
    let severity = serde_json::to_string(&spec.severity)
        .unwrap_or_default()
        .trim_matches('"')
        .to_string();
    (
        spec.id.clone(),
        language,
        dimension,
        severity,
        spec.message.clone(),
        source,
    )
}
enum FileOutcome {
    Scanned {
        file_findings: Vec<Finding>,
        file_events: Vec<ScanEvent>,
        normalized_lines: Option<Vec<(String, usize)>>,
        taint_facts: Vec<(String, crate::taint::FileFacts)>,
    },
    EventOnly(Vec<ScanEvent>),
    Cancelled,
}

fn run_ast_rule(
    rule: &AstRule,
    _lang: Language,
    tree: &tree_sitter::Tree,
    source: &str,
    path: &str,
    degraded: bool,
) -> Vec<Finding> {
    let root = tree.root_node();
    let mut cursor = tree_sitter::QueryCursor::new();
    let mut findings = Vec::new();

    use tree_sitter::StreamingIterator as _;

    let mut not_ranges: Vec<(usize, usize)> = Vec::new();
    for neg in &rule.not_queries {
        let mut it = cursor.matches(neg, root, source.as_bytes());
        while let Some(m) = it.next() {
            if let Some((s, e)) = match_range(m) {
                not_ranges.push((s, e));
            }
        }
    }
    let mut inside_ranges: Vec<(usize, usize)> = Vec::new();
    for ins in &rule.inside_queries {
        let mut it = cursor.matches(ins, root, source.as_bytes());
        while let Some(m) = it.next() {
            if let Some((s, e)) = match_range(m) {
                inside_ranges.push((s, e));
            }
        }
    }

    let names: Vec<String> = rule
        .query
        .capture_names()
        .iter()
        .map(|n| n.to_string())
        .collect();

    let mut it = cursor.matches(&rule.query, root, source.as_bytes());
    while let Some(m) = it.next() {
        let mut captures: Vec<(String, String)> = Vec::new();
        // (start_byte, end_byte, start_point, end_point)
        let mut primary: Option<(usize, usize, tree_sitter::Point, tree_sitter::Point)> = None;
        for cap in m.captures {
            let name = names
                .get(cap.index as usize)
                .cloned()
                .unwrap_or_default();
            let text = source
                .get(cap.node.start_byte()..cap.node.end_byte())
                .unwrap_or("")
                .to_string();
            if name == "match" && primary.is_none() {
                primary = Some((
                    cap.node.start_byte(),
                    cap.node.end_byte(),
                    cap.node.start_position(),
                    cap.node.end_position(),
                ));
            }
            captures.push((name, text));
        }
        let Some((start, end, sp, ep)) = primary else {
            continue;
        };
        let match_text = source
            .get(start..end.min(source.len()))
            .unwrap_or("")
            .to_string();

        // not：同位或包含 → 排除。
        if not_ranges
            .iter()
            .any(|(ns, ne)| *ns <= start && *ne >= end)
        {
            continue;
        }
        // inside：必须被某命中包含。
        if !rule.inside_queries.is_empty()
            && !inside_ranges
                .iter()
                .any(|(ns, ne)| *ns <= start && *ne >= end)
        {
            continue;
        }
        // 元变量约束：捕获缺失或不匹配 → 排除。
        let mut ok = true;
        for (capture, regex) in &rule.metavariables {
            match captures.iter().find(|(n, _)| n == capture) {
                Some((_, text)) => {
                    if !regex.is_match(text) {
                        ok = false;
                        break;
                    }
                }
                None => {
                    ok = false;
                    break;
                }
            }
        }
        if !ok {
            continue;
        }

        captures.sort_by(|a, b| a.0.cmp(&b.0));
        findings.push(Finding {
            rule_id: rule.spec.id.clone(),
            dimension: rule.spec.dimension,
            severity: rule.spec.severity,
            path: path.to_string(),
            span: Span {
                start: Position {
                    line: sp.row + 1,
                    column: sp.column + 1,
                },
                end: Position {
                    line: ep.row + 1,
                    column: ep.column + 1,
                },
            },
            message: rule.spec.message.clone(),
            match_text,
            captures,
            degraded,
            flows: Vec::new(),
            fingerprint: String::new(),
        });
    }
    findings
}

/// 命中的整体字节范围（所有捕获节点的最小/最大）。
fn match_range(m: &tree_sitter::QueryMatch<'_, '_>) -> Option<(usize, usize)> {
    let mut start = usize::MAX;
    let mut end = 0usize;
    for cap in m.captures {
        start = start.min(cap.node.start_byte());
        end = end.max(cap.node.end_byte());
    }
    if start == usize::MAX {
        None
    } else {
        Some((start, end))
    }
}

fn run_text_rule(rule: &TextRule, source: &str, path: &str, degraded: bool) -> Vec<Finding> {
    let line_starts: Vec<usize> = std::iter::once(0usize)
        .chain(
            source
                .match_indices('\n')
                .map(|(i, _)| i + 1),
        )
        .collect();

    let locate = |byte: usize| -> (usize, usize) {
        let line_idx = match line_starts.binary_search(&byte) {
            Ok(i) => i,
            Err(i) => i.saturating_sub(1),
        };
        (line_idx + 1, byte - line_starts[line_idx] + 1)
    };

    let mut findings = Vec::new();
    for m in rule.regex.find_iter(source) {
        let (start_line, start_col) = locate(m.start());
        let (end_line, end_col) = locate(m.end());
        findings.push(Finding {
            rule_id: rule.spec.id.clone(),
            dimension: rule.spec.dimension,
            severity: rule.spec.severity,
            path: path.to_string(),
            span: Span {
                start: Position {
                    line: start_line,
                    column: start_col,
                },
                end: Position {
                    line: end_line,
                    column: end_col,
                },
            },
            message: rule.spec.message.clone(),
            match_text: m.as_str().to_string(),
            captures: Vec::new(),
            degraded,
            flows: Vec::new(),
            fingerprint: String::new(),
        });
    }
    findings
}

/// `ignore` 遍历：尊重 .gitignore（无需 .git 存在）、`.codescanignore`、
/// 全局忽略文件与内置默认排除目录；跳过隐藏文件、不跟随符号链接。
/// 按与扫描相同的忽略边界列出工作区文件（绝对路径, 相对路径），按相对路径排序。
/// 供扫描之外的只读消费者（如 LLM 源码审查）复用同一文件集合。
pub fn list_workspace_files(root: &Path, options: &ScanOptions) -> Vec<(PathBuf, String)> {
    let mut files = walk_workspace(root, options);
    files.sort_by(|a, b| a.1.cmp(&b.1));
    files
}

fn walk_workspace(root: &Path, options: &ScanOptions) -> Vec<(PathBuf, String)> {
    let mut out = Vec::new();
    let mut builder = ignore::WalkBuilder::new(root);
    builder
        .hidden(true)
        .git_ignore(true)
        .git_exclude(true)
        .require_git(false)
        .follow_links(false)
        .add_custom_ignore_filename(".codescanignore");
    if let Some(global) = &options.global_ignore_file {
        if global.is_file() {
            builder.add_ignore(global);
        }
    }
    // 内置默认排除目录（best-effort：非法模式忽略）。
    let mut overrides = ignore::overrides::OverrideBuilder::new(root);
    for dir in DEFAULT_IGNORED_DIRS {
        let _ = overrides.add(&format!("!{dir}/"));
    }
    if let Ok(overrides) = overrides.build() {
        builder.overrides(overrides);
    }
    let walker = builder.build();
    for entry in walker.flatten() {
        if entry.file_type().map(|t| t.is_file()).unwrap_or(false) {
            let abs = entry.path().to_path_buf();
            let rel = abs
                .strip_prefix(root)
                .map(|p| p.to_string_lossy().replace('\\', "/"))
                .unwrap_or_else(|_| abs.to_string_lossy().into_owned());
            out.push((abs, rel));
        }
    }
    out
}

/// 原始文件计数（含被忽略者；不进 .git、不跟随符号链接），用于 IgnoredSummary。
fn count_all_files(root: &Path) -> u64 {
    fn rec(dir: &Path, count: &mut u64) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            if file_type.is_symlink() {
                continue;
            }
            if file_type.is_dir() {
                if entry.file_name() == ".git" {
                    continue;
                }
                rec(&entry.path(), count);
            } else if file_type.is_file() {
                *count += 1;
            }
        }
    }
    let mut count = 0u64;
    rec(root, &mut count);
    count
}

