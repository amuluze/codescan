//! 扫描编排：walk → classify → parse → 匹配 → 度量 → 指纹/排序。
//! 确定性（Domain §2）：文件列表排序后处理，rayon 仅做无共享并行，
//! 汇总后统一排序；事件与发现均不携带机器差异。

use crate::compile::{AstRule, CompiledRule, InheritanceRule, MetricRule, TextRule};
use crate::fingerprint;
use crate::language::{self, Language};
use crate::metrics;
use crate::model::{
    CancelToken, Finding, Progress, ScanError, ScanEvent, ScanReport, ScanStats, Span, Position,
};
use crate::rules::{self, RuleError, RuleLanguage, RuleSpec};
use rayon::prelude::*;
use regex::Regex;
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
    ".git",
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
    metric_rules: Vec<MetricRule>,
    taint_rules: Vec<crate::compile::TaintRule>,
    inheritance_rules: Vec<InheritanceRule>,
    /// 全部规则声明（内置 + 自定义，含已禁用），用于规则目录与覆盖性检查。
    all_specs: Vec<RuleSpec>,
    /// 已禁用规则的解析/编译错误（不阻断扫描，供管理面展示）。
    rule_errors: Vec<RuleError>,
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
        let (custom, custom_errors) = match custom_yaml {
            Some(yaml) => rules::parse_rule_set_lenient(yaml),
            None => (Vec::new(), Vec::new()),
        };
        // 禁用规则的结构/编译错误不阻断扫描；错误留在 Engine 供管理面读取。
        let mut rule_errors = Vec::new();
        let mut fatal_errors = Vec::new();
        for error in custom_errors {
            if disabled.contains(&error.rule_id) {
                rule_errors.push(error);
            } else {
                fatal_errors.push(error);
            }
        }
        if !fatal_errors.is_empty() {
            return Err(fatal_errors);
        }
        let merged = rules::merge_rule_sets(builtin, custom)?;
        let all_specs = merged.clone();
        // 编译校验范围 = 启用集（Domain §3.3）；禁用规则保留在规则目录中。
        let enabled: Vec<RuleSpec> = merged
            .into_iter()
            .filter(|spec| !disabled.contains(&spec.id))
            .collect();
        let compiled = crate::compile::compile(enabled)?;

        let mut ast_rules = Vec::new();
        let mut text_rules = Vec::new();
        let mut metric_rules = Vec::new();
        let mut taint_rules = Vec::new();
        let mut inheritance_rules = Vec::new();
        for rule in compiled {
            match rule {
                CompiledRule::Ast(r) => ast_rules.push(r),
                CompiledRule::Text(r) => text_rules.push(r),
                CompiledRule::Metric(r) => metric_rules.push(r),
                CompiledRule::Taint(r) => taint_rules.push(r),
                CompiledRule::Inheritance(r) => inheritance_rules.push(r),
            }
        }

        Ok(Engine {
            ast_rules,
            text_rules,
            metric_rules,
            taint_rules,
            inheritance_rules,
            all_specs,
            rule_errors,
            engine_version: env!("CARGO_PKG_VERSION").to_string(),
        })
    }

    /// 规则目录（UI 规则管理页）：(id, language, dimension, severity, message, source)。
    /// 含已禁用规则；source 为规则体的可读重建（query / 正则 / 度量名 / 污点端点计数）。
    pub fn rule_catalog(&self) -> Vec<(String, String, String, String, String, String)> {
        let mut out: Vec<_> = self
            .all_specs
            .iter()
            .map(|spec| catalog_entry(spec, catalog_source(spec)))
            .collect();
        out.sort_by(|a, b| a.0.cmp(&b.0));
        out
    }

    /// 全部规则 id（内置 + 自定义，启停无关，用于覆盖性检查）。
    pub fn all_rule_ids(&self) -> Vec<String> {
        let mut ids: Vec<String> = self.all_specs.iter().map(|spec| spec.id.clone()).collect();
        ids.sort();
        ids
    }

    /// 本次实际参与扫描的规则 id（启用且编译成功）。
    pub fn enabled_rule_ids(&self) -> Vec<String> {
        let mut ids: Vec<String> = self
            .ast_rules
            .iter()
            .map(|rule| rule.spec.id.clone())
            .chain(self.text_rules.iter().map(|rule| rule.spec.id.clone()))
            .chain(self.metric_rules.iter().map(|rule| rule.spec.id.clone()))
            .chain(self.taint_rules.iter().map(|rule| rule.spec.id.clone()))
            .chain(
                self.inheritance_rules
                    .iter()
                    .map(|rule| rule.spec.id.clone()),
            )
            .collect();
        ids.sort();
        ids.dedup();
        ids
    }

    /// 已禁用规则被跳过的解析/编译错误，供规则管理面展示。
    pub fn rule_errors(&self) -> &[RuleError] {
        &self.rule_errors
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
        let mut corpus: Vec<crate::metrics::CorpusFile> = Vec::new();
        let mut taint_facts: std::collections::HashMap<
            String,
            Vec<crate::taint::FileFacts>,
        > = std::collections::HashMap::new();
        let mut type_facts: Vec<crate::types::TypeFacts> = Vec::new();
        let mut scanned = 0u64;

        for (outcome, (_, rel)) in results.into_iter().zip(files.iter()) {
            match outcome {
                FileOutcome::Scanned {
                    mut file_findings,
                    mut file_events,
                    normalized_lines,
                    test_ranges,
                    taint_facts: file_taint,
                    type_facts: file_types,
                } => {
                    scanned += 1;
                    if let Some(lines) = normalized_lines {
                        corpus.push(crate::metrics::CorpusFile {
                            path: rel.clone(),
                            lines,
                            test_ranges,
                        });
                    }
                    findings.append(&mut file_findings);
                    events.append(&mut file_events);
                    for (rule_id, facts) in file_taint {
                        taint_facts.entry(rule_id).or_default().push(facts);
                    }
                    if let Some(facts) = file_types {
                        type_facts.push(*facts);
                    }
                }
                FileOutcome::EventOnly(mut file_events) => {
                    events.append(&mut file_events);
                }
                FileOutcome::Cancelled => unreachable!("取消已在上方短路"),
            }
        }

        // 语料级度量：重复块。
        findings.extend(metrics::duplicate_blocks(&corpus, &self.metric_rules));

        // 污点：项目级分析（显式导入跨文件 + 字段/对象组；文件顺序由 facts 收集顺序保证）。
        for rule in &self.taint_rules {
            if let Some(facts) = taint_facts.get(&rule.spec.id) {
                findings.extend(crate::taint::analyze_project(rule, facts));
            }
        }

        // v2 类型层：Java 继承闭包分析（项目级，文件顺序保持 facts 收集顺序）。
        for rule in &self.inheritance_rules {
            findings.extend(crate::types::analyze(rule, &type_facts));
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
        let Some(lang) = Language::detect(abs) else {
            // 未知扩展名不进入 AST/污点/度量管线，但文本规则（密钥、TODO 等）
            // 仍作用于所有非二进制文本文件（含 `.env`、YAML、JSON 等）。
            unknown_counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            let source = String::from_utf8_lossy(&bytes).into_owned();
            let mut findings = Vec::new();
            for rule in &self.text_rules {
                if path_excluded(&rule.exclude_paths, rel) {
                    continue;
                }
                findings.extend(run_text_rule(rule, &source, rel, false));
            }
            return FileOutcome::Scanned {
                file_findings: findings,
                file_events: Vec::new(),
                normalized_lines: None,
                test_ranges: Vec::new(),
                taint_facts: Vec::new(),
                type_facts: None,
            };
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
        let file_imports = if self.ast_rules.iter().any(|rule| !rule.imports.is_empty())
            || self.taint_rules.iter().any(|rule| !rule.imports.is_empty())
        {
            collect_import_texts(lang, tree, source)
        } else {
            Vec::new()
        };
        let file_aliases = if self
            .ast_rules
            .iter()
            .any(|rule| !rule.imported_symbols.is_empty())
            || self
                .taint_rules
                .iter()
                .any(|rule| !rule.spec.imported_symbols.is_empty())
        {
            collect_import_aliases(lang, tree, source)
        } else {
            Vec::new()
        };
        let local_definitions = if self.ast_rules.iter().any(|rule| rule.no_shadow) {
            collect_local_definitions(tree, source)
        } else {
            HashSet::new()
        };

        let ast_context = AstFileContext {
            tree,
            source,
            path: rel,
            degraded,
            imports: &file_imports,
            aliases: &file_aliases,
            definitions: &local_definitions,
        };
        for rule in &self.ast_rules {
            if let RuleLanguage::One(l) = rule.spec.language {
                if l != lang {
                    continue;
                }
                if path_excluded(&rule.exclude_paths, rel) {
                    continue;
                }
                findings.extend(run_ast_rule(rule, &ast_context));
            }
        }
        for rule in &self.text_rules {
            if path_excluded(&rule.exclude_paths, rel) {
                continue;
            }
            findings.extend(run_text_rule(rule, source, rel, degraded));
        }
        let mut taint_facts: Vec<(String, crate::taint::FileFacts)> = Vec::new();
        for rule in &self.taint_rules {
            if let RuleLanguage::One(l) = rule.spec.language {
                if l != lang {
                    continue;
                }
                if !imports_match(&rule.imports, &file_imports) {
                    continue;
                }
                taint_facts.push((
                    rule.spec.id.clone(),
                    crate::taint::extract_file(rule, lang, tree, source, rel, degraded, &file_aliases),
                ));
            }
        }
        let test_ranges = test_code_ranges(lang, tree, rel, source);
        findings.extend(metrics::file_metrics_with_tests(
            lang,
            tree,
            source,
            rel,
            degraded,
            &self.metric_rules,
            &test_ranges,
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

        let type_facts = if self.inheritance_rules.is_empty() {
            None
        } else {
            crate::types::extract_file(lang, tree, source, rel, degraded).map(Box::new)
        };

        FileOutcome::Scanned {
            file_findings: findings,
            file_events: events,
            normalized_lines: Some(normalized_lines),
            test_ranges,
            taint_facts,
            type_facts,
        }
    }
}


fn catalog_source(spec: &RuleSpec) -> String {
    if let Some(pattern) = &spec.pattern {
        return pattern.clone();
    }
    if let Some(text) = &spec.text {
        return text.clone();
    }
    if let Some(metric) = &spec.metric {
        let options = spec
            .options
            .iter()
            .map(|(key, value)| format!("{key}={value}"))
            .collect::<Vec<_>>()
            .join(" ");
        return format!("{metric} {options}").trim_end().to_string();
    }
    if let Some(taint) = &spec.taint {
        return format!(
            "污点：{} source → {} sink（sanitizer {}）",
            taint.sources.len(),
            taint.sinks.len(),
            taint.sanitizers.len()
        );
    }
    if let Some(inheritance) = &spec.inheritance {
        return format!(
            "继承：{}（方法 {}）",
            inheritance.target,
            inheritance.methods.join(", ")
        );
    }
    String::new()
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
        test_ranges: Vec<(usize, usize)>,
        taint_facts: Vec<(String, crate::taint::FileFacts)>,
        type_facts: Option<Box<crate::types::TypeFacts>>,
    },
    EventOnly(Vec<ScanEvent>),
    Cancelled,
}

fn collect_import_texts(
    lang: Language,
    tree: &tree_sitter::Tree,
    source: &str,
) -> Vec<String> {
    let mut texts = Vec::new();
    crate::metrics::walk_all_pub(tree.root_node(), &mut |node| {
        let include = match lang {
            Language::TypeScript | Language::Tsx | Language::JavaScript | Language::Jsx => {
                node.kind() == "import_statement"
                    || (node.kind() == "call_expression"
                        && node
                            .child_by_field_name("function")
                            .and_then(|function| {
                                source.get(function.start_byte()..function.end_byte())
                            })
                            .is_some_and(|function| {
                                matches!(function, "require" | "import")
                            }))
            }
            Language::Python => matches!(
                node.kind(),
                "import_statement" | "import_from_statement" | "future_import_statement"
            ),
            Language::Java => node.kind() == "import_declaration",
            Language::Go => node.kind() == "import_spec",
            Language::Rust => node.kind() == "use_declaration",
        };
        if include {
            if let Some(text) = source.get(node.start_byte()..node.end_byte()) {
                texts.push(text.to_string());
            }
        }
    });
    texts.sort();
    texts.dedup();
    texts
}

fn collect_import_aliases(
    lang: Language,
    tree: &tree_sitter::Tree,
    source: &str,
) -> Vec<(String, String)> {
    let mut aliases = Vec::new();
    crate::metrics::walk_all_pub(tree.root_node(), &mut |node| match lang {
        Language::TypeScript | Language::Tsx | Language::JavaScript | Language::Jsx => {
            if node.kind() == "import_statement" {
                let module = node
                    .child_by_field_name("source")
                    .and_then(|source_node| source.get(source_node.start_byte()..source_node.end_byte()))
                    .map(|module| module.trim_matches(|c| c == '"' || c == '\'' || c == '`'));
                if let Some(module) = module {
                    if let Some(clause) = node
                        .child_by_field_name("import_clause")
                        .or_else(|| crate::taint::named_children(node).into_iter().find(|child| child.kind() == "import_clause"))
                    {
                        for child in crate::taint::named_children(clause) {
                            if child.kind() == "namespace_import" {
                                if let Some(local) = crate::taint::named_children(child)
                                    .into_iter()
                                    .find(|name| name.kind() == "identifier")
                                    .and_then(|name| source.get(name.start_byte()..name.end_byte()))
                                {
                                    aliases.push((local.to_string(), module.to_string()));
                                }
                            } else if child.kind() == "identifier" {
                                if let Some(local) = source.get(child.start_byte()..child.end_byte()) {
                                    aliases.push((local.to_string(), module.to_string()));
                                }
                            }
                        }
                    }
                }
            }
            if node.kind() == "variable_declarator" {
                let Some(name) = node
                    .child_by_field_name("name")
                    .filter(|name| name.kind() == "identifier")
                    .and_then(|name| source.get(name.start_byte()..name.end_byte()))
                else {
                    return;
                };
                let Some(value) = node.child_by_field_name("value") else {
                    return;
                };
                if value.kind() != "call_expression" {
                    return;
                }
                let function = value
                    .child_by_field_name("function")
                    .and_then(|function| source.get(function.start_byte()..function.end_byte()));
                if function != Some("require") {
                    return;
                }
                let module = value
                    .child_by_field_name("arguments")
                    .and_then(|arguments| crate::taint::named_children(arguments).into_iter().next())
                    .and_then(|argument| source.get(argument.start_byte()..argument.end_byte()))
                    .map(|module| module.trim_matches(|c| c == '"' || c == '\'' || c == '`'));
                if let Some(module) = module {
                    aliases.push((name.to_string(), module.to_string()));
                }
            }
            if node.kind() == "import_specifier" {
                let Some(original) = node
                    .child_by_field_name("name")
                    .and_then(|name| source.get(name.start_byte()..name.end_byte()))
                else {
                    return;
                };
                let local = node
                    .child_by_field_name("alias")
                    .and_then(|alias| source.get(alias.start_byte()..alias.end_byte()))
                    .unwrap_or(original);
                if local != original {
                    aliases.push((local.to_string(), original.to_string()));
                }
            }
            if node.kind() == "pair_pattern" {
                let Some(key) = node.child_by_field_name("key") else {
                    return;
                };
                let Some(value) = node.child_by_field_name("value") else {
                    return;
                };
                if !matches!(key.kind(), "property_identifier" | "identifier")
                    || value.kind() != "identifier"
                {
                    return;
                }
                let (Some(original), Some(local)) = (
                    source.get(key.start_byte()..key.end_byte()),
                    source.get(value.start_byte()..value.end_byte()),
                ) else {
                    return;
                };
                if local != original {
                    aliases.push((local.to_string(), original.to_string()));
                }
            }
        }
        Language::Python => {
            if node.kind() == "aliased_import" {
                let Some(original) = node
                    .child_by_field_name("name")
                    .and_then(|name| source.get(name.start_byte()..name.end_byte()))
                else {
                    return;
                };
                let Some(local) = node
                    .child_by_field_name("alias")
                    .and_then(|alias| source.get(alias.start_byte()..alias.end_byte()))
                else {
                    return;
                };
                if local != original {
                    aliases.push((local.to_string(), original.to_string()));
                }
            }
        }
        Language::Go => {
            if node.kind() == "import_spec" {
                let Some(path) = node
                    .child_by_field_name("path")
                    .and_then(|path| source.get(path.start_byte()..path.end_byte()))
                else {
                    return;
                };
                let original = go_package_name(path);
                let Some(local) = node
                    .child_by_field_name("name")
                    .filter(|name| name.kind() == "package_identifier")
                    .and_then(|name| source.get(name.start_byte()..name.end_byte()))
                else {
                    return;
                };
                if local != original {
                    aliases.push((local.to_string(), original));
                }
            }
        }
        Language::Rust => {
            if node.kind() == "use_declaration" {
                if let Some(argument) = node.child_by_field_name("argument") {
                    collect_rust_use_aliases(argument, source, &[], &mut aliases);
                }
            }
        }
        Language::Java => {}
    });
    aliases.sort();
    aliases.dedup();
    aliases
}

fn collect_rust_use_aliases(
    node: tree_sitter::Node<'_>,
    source: &str,
    prefix: &[String],
    aliases: &mut Vec<(String, String)>,
) {
    match node.kind() {
        "use_as_clause" => {
            let Some(path) = node.child_by_field_name("path") else {
                return;
            };
            let Some(alias) = node
                .child_by_field_name("alias")
                .and_then(|alias| source.get(alias.start_byte()..alias.end_byte()))
            else {
                return;
            };
            let Some(path_text) = source.get(path.start_byte()..path.end_byte()) else {
                return;
            };
            let mut full: Vec<String> = prefix.to_vec();
            if path_text.trim() != "self" {
                full.extend(
                    path_text
                        .split("::")
                        .map(|segment| segment.trim().to_string())
                        .filter(|segment| !segment.is_empty()),
                );
            }
            if alias == "_" || full.is_empty() {
                return;
            }
            let original = full.join("::");
            if alias != original {
                aliases.push((alias.to_string(), original));
            }
        }
        "scoped_use_list" => {
            let Some(path) = node.child_by_field_name("path") else {
                return;
            };
            let Some(list) = node.child_by_field_name("list") else {
                return;
            };
            let Some(path_text) = source.get(path.start_byte()..path.end_byte()) else {
                return;
            };
            let mut next = prefix.to_vec();
            next.extend(
                path_text
                    .split("::")
                    .map(|segment| segment.trim().to_string())
                    .filter(|segment| !segment.is_empty()),
            );
            for child in crate::taint::named_children(list) {
                collect_rust_use_aliases(child, source, &next, aliases);
            }
        }
        "use_list" => {
            for child in crate::taint::named_children(node) {
                collect_rust_use_aliases(child, source, prefix, aliases);
            }
        }
        _ => {}
    }
}

fn go_package_name(path: &str) -> String {
    let path = path.trim_matches(|c| c == '"' || c == '`');
    let mut segments: Vec<&str> = path.split('/').filter(|segment| !segment.is_empty()).collect();
    if let Some(last) = segments.last() {
        let version = last
            .strip_prefix('v')
            .is_some_and(|suffix| !suffix.is_empty() && suffix.chars().all(|c| c.is_ascii_digit()));
        if version && segments.len() > 1 {
            segments.pop();
        }
    }
    segments.last().copied().unwrap_or(path).to_string()
}

fn imports_match(imports: &[Regex], file_imports: &[String]) -> bool {
    imports.is_empty()
        || imports
            .iter()
            .any(|pattern| file_imports.iter().any(|text| pattern.is_match(text)))
}

struct AstFileContext<'a> {
    tree: &'a tree_sitter::Tree,
    source: &'a str,
    path: &'a str,
    degraded: bool,
    imports: &'a [String],
    aliases: &'a [(String, String)],
    definitions: &'a HashSet<String>,
}

fn collect_local_definitions(tree: &tree_sitter::Tree, source: &str) -> HashSet<String> {
    let mut names = HashSet::new();
    crate::metrics::walk_all_pub(tree.root_node(), &mut |node| match node.kind() {
        "function_declaration" | "function_definition" => {
            if let Some(name) = node.child_by_field_name("name") {
                if let Some(text) = source.get(name.start_byte()..name.end_byte()) {
                    names.insert(text.to_string());
                }
            }
        }
        "variable_declarator" => {
            if node
                .child_by_field_name("value")
                .and_then(|value| source.get(value.start_byte()..value.end_byte()))
                .is_some_and(|text| text.contains("require"))
            {
                return;
            }
            if let Some(name) = node
                .child_by_field_name("name")
                .filter(|name| name.kind() == "identifier")
            {
                if let Some(text) = source.get(name.start_byte()..name.end_byte()) {
                    names.insert(text.to_string());
                }
            }
        }
        _ => {}
    });
    names
}

fn run_ast_rule(rule: &AstRule, context: &AstFileContext<'_>) -> Vec<Finding> {
    let tree = context.tree;
    let source = context.source;
    let path = context.path;
    let degraded = context.degraded;
    let file_imports = context.imports;
    let file_aliases = context.aliases;
    let local_definitions = context.definitions;
    if !imports_match(&rule.imports, file_imports) {
        return Vec::new();
    }
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
        for cap in m.captures() {
            let name = names
                .get(cap.index as usize)
                .cloned()
                .unwrap_or_default();
            let text = source
                .get(cap.node.start_byte()..cap.node.end_byte())
                .unwrap_or("")
                .to_string();
            if name == "match" {
                let candidate = (
                    cap.node.start_byte(),
                    cap.node.end_byte(),
                    cap.node.start_position(),
                    cap.node.end_position(),
                );
                let replace = match primary {
                    None => true,
                    Some((start, end, _, _)) => {
                        (candidate.1 - candidate.0, candidate.0) < (end - start, start)
                    }
                };
                if replace {
                    primary = Some(candidate);
                }
            }
            captures.push((name, text));
        }
        let Some((start, end, sp, ep)) = primary else {
            continue;
        };
        if rule.spec.field_receiver {
            let Some((_, receiver)) = captures.iter().find(|(name, _)| name == "obj") else {
                continue;
            };
            if !receiver_is_ois_field(root, start, receiver, source) {
                continue;
            }
        }
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
            let mut matched = false;
            for (name, text) in &captures {
                if name == capture {
                    matched = true;
                    let alias_match = rule.spec.imported_symbols.iter().any(|symbol| {
                        file_aliases.iter().any(|(local, original)| {
                            local == text
                                && original == symbol
                                && regex.is_match(original)
                        })
                    });
                    if !regex.is_match(text) && !alias_match {
                        ok = false;
                        break;
                    }
                    if rule.no_shadow && local_definitions.contains(text) {
                        ok = false;
                        break;
                    }
                }
            }
            if !ok || !matched {
                ok = false;
                break;
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

/// Java：匹配点在 `name` 对应的 ObjectInputStream 字段所在的类内。
fn receiver_is_ois_field(
    root: tree_sitter::Node<'_>,
    match_start: usize,
    name: &str,
    source: &str,
) -> bool {
    let mut enclosing: Option<tree_sitter::Node<'_>> = None;
    crate::metrics::walk_all_pub(root, &mut |node| {
        if node.kind() == "class_declaration"
            && node.start_byte() <= match_start
            && match_start < node.end_byte()
        {
            let better = enclosing.is_none_or(|current| {
                node.end_byte() - node.start_byte() < current.end_byte() - current.start_byte()
            });
            if better {
                enclosing = Some(node);
            }
        }
    });
    let Some(class) = enclosing else {
        return false;
    };
    let Some(body) = class.child_by_field_name("body") else {
        return false;
    };
    let mut found = false;
    crate::metrics::walk_all_pub(body, &mut |node| {
        if found || node.kind() != "field_declaration" {
            return;
        }
        let Some(type_text) = node
            .child_by_field_name("type")
            .and_then(|ty| source.get(ty.start_byte()..ty.end_byte()))
        else {
            return;
        };
        if !type_text.contains("ObjectInputStream") {
            return;
        }
        crate::metrics::walk_all_pub(node, &mut |child| {
            if found || child.kind() != "variable_declarator" {
                return;
            }
            let Some(field) = child
                .child_by_field_name("name")
                .and_then(|identifier| source.get(identifier.start_byte()..identifier.end_byte()))
            else {
                return;
            };
            if field == name {
                found = true;
            }
        });
    });
    found
}

/// 命中的整体字节范围（所有捕获节点的最小/最大）。
fn match_range(m: &tree_sitter::QueryMatch<'_, '_>) -> Option<(usize, usize)> {
    let mut start = usize::MAX;
    let mut end = 0usize;
    for cap in m.captures() {
        start = start.min(cap.node.start_byte());
        end = end.max(cap.node.end_byte());
    }
    if start == usize::MAX {
        None
    } else {
        Some((start, end))
    }
}

fn test_code_ranges(
    lang: Language,
    tree: &tree_sitter::Tree,
    rel: &str,
    source: &str,
) -> Vec<(usize, usize)> {
    let line_count = source.lines().count().max(1);
    if is_test_path(rel) {
        return vec![(1, line_count)];
    }
    if lang != Language::Rust {
        return Vec::new();
    }
    let mut ranges = Vec::new();
    crate::metrics::walk_all_pub(tree.root_node(), &mut |node| {
        if node.kind() != "mod_item" {
            return;
        }
        let is_tests = node
            .child_by_field_name("name")
            .and_then(|name| source.get(name.start_byte()..name.end_byte()))
            .is_some_and(|name| name == "tests");
        if is_tests {
            ranges.push((
                node.start_position().row + 1,
                node.end_position().row + 1,
            ));
        }
    });
    ranges
}

fn is_test_path(rel: &str) -> bool {
    let name = rel.rsplit('/').next().unwrap_or(rel);
    name.contains(".test.")
        || name.contains(".spec.")
        || name.starts_with("test_")
        || name.ends_with("_test.rs")
        || rel.rsplit('/').nth(1) == Some("tests")
}

fn path_excluded(exclude_paths: &[regex::Regex], path: &str) -> bool {
    exclude_paths.iter().any(|pattern| pattern.is_match(path))
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
        if rule
            .exclude
            .as_ref()
            .is_some_and(|exclude| exclude.is_match(m.as_str()))
        {
            continue;
        }
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
/// 全局忽略文件与内置默认排除目录；扫描隐藏源码（如 `.env`/`.config/`），
/// 但始终排除 `.git`；不跟随符号链接。
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
        .hidden(false)
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

