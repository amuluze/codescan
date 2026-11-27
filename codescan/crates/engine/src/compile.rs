//! 规则编译：RuleSpec → 可执行形态。query 语法错误、正则错误、
//! 元变量引用不存在的捕获，都在此阶段报为 RuleError（Domain §3.3）。

use crate::rules::{RuleError, RuleLanguage, RuleSpec};
use regex::Regex;
use tree_sitter::Query;

pub struct AstRule {
    pub spec: RuleSpec,
    pub query: Query,
    pub not_queries: Vec<Query>,
    pub inside_queries: Vec<Query>,
    /// (捕获名, 正则)：该捕获的文本必须匹配正则才保留命中。
    pub metavariables: Vec<(String, Regex)>,
    /// 可选导入条件：文件 import 声明至少命中一条正则。
    pub imports: Vec<Regex>,
    /// 允许以导入别名满足 metavariable 的原始符号名。
    pub imported_symbols: Vec<String>,
    pub no_shadow: bool,
    pub exclude_paths: Vec<Regex>,
}

pub struct TextRule {
    pub spec: RuleSpec,
    pub regex: Regex,
    pub exclude: Option<Regex>,
    pub exclude_paths: Vec<Regex>,
}

/// 度量配置：kind 由 metrics.rs 解释，阈值来自 options。
pub struct MetricRule {
    pub spec: RuleSpec,
    pub kind: MetricKind,
    pub options: Vec<(String, f64)>,
}

pub struct TaintRule {
    pub spec: RuleSpec,
    pub source_queries: Vec<Query>,
    pub sink_queries: Vec<Query>,
    pub sanitizer_queries: Vec<Query>,
    /// 可选导入条件：文件 import 声明至少命中一条正则。
    pub imports: Vec<Regex>,
}

/// 类型继承规则（v2 类型层）：目标类型 + 方法名 + 可选空实现/间接实现约束。
pub struct InheritanceRule {
    pub spec: RuleSpec,
    pub target: String,
    pub methods: Vec<String>,
    pub empty_only: bool,
    pub indirect_only: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MetricKind {
    CyclomaticComplexity,
    FunctionLength,
    NestingDepth,
    DuplicateBlock,
    UnreachableCode,
}

impl MetricKind {
    fn parse(value: &str) -> Option<MetricKind> {
        match value {
            "cyclomatic-complexity" => Some(MetricKind::CyclomaticComplexity),
            "function-length" => Some(MetricKind::FunctionLength),
            "nesting-depth" => Some(MetricKind::NestingDepth),
            "duplicate-block" => Some(MetricKind::DuplicateBlock),
            "unreachable-code" => Some(MetricKind::UnreachableCode),
            _ => None,
        }
    }
}

/// @match 捕获决定发现 span；缺失即规则无法产出可定位发现。
fn query_has_match(query: &Query) -> bool {
    query.capture_names().contains(&"match")
}

fn compile_path_excludes(
    spec: &RuleSpec,
    errors: &mut Vec<RuleError>,
) -> Vec<Regex> {
    spec.exclude_paths
        .iter()
        .filter_map(|pattern| match Regex::new(pattern) {
            Ok(regex) => Some(regex),
            Err(error) => {
                errors.push(RuleError {
                    rule_id: spec.id.clone(),
                    field: "exclude_paths".into(),
                    message: error.to_string(),
                });
                None
            }
        })
        .collect()
}

fn compile_metric(spec: &RuleSpec, metric: &str) -> Result<MetricRule, Vec<RuleError>> {
    let Some(kind) = MetricKind::parse(metric) else {
        return Err(vec![RuleError {
            rule_id: spec.id.clone(),
            field: "metric".into(),
            message: format!("未知度量 {metric:?}"),
        }]);
    };
    let allowed: &[&str] = match kind {
        MetricKind::CyclomaticComplexity | MetricKind::FunctionLength | MetricKind::NestingDepth => {
            &["threshold", "skip_tests"]
        }
        MetricKind::DuplicateBlock => &["window", "min_occurrences", "skip_tests"],
        MetricKind::UnreachableCode => &["skip_tests"],
    };
    let mut errors = Vec::new();
    for (name, value) in &spec.options {
        if !allowed.contains(&name.as_str()) {
            errors.push(RuleError {
                rule_id: spec.id.clone(),
                field: "options".into(),
                message: format!("度量 {metric} 不支持 option {name:?}"),
            });
            continue;
        }
        if !value.is_finite() || *value < 0.0 {
            errors.push(RuleError {
                rule_id: spec.id.clone(),
                field: "options".into(),
                message: format!("option {name} 必须是非负有限数"),
            });
        }
    }
    if kind == MetricKind::DuplicateBlock {
        let window = spec
            .options
            .iter()
            .find(|(name, _)| name == "window")
            .map(|(_, value)| *value)
            .unwrap_or(10.0);
        if window < 1.0 {
            errors.push(RuleError {
                rule_id: spec.id.clone(),
                field: "options.window".into(),
                message: "window 必须 ≥ 1".into(),
            });
        }
        let min_occurrences = spec
            .options
            .iter()
            .find(|(name, _)| name == "min_occurrences")
            .map(|(_, value)| *value)
            .unwrap_or(2.0);
        if min_occurrences < 2.0 {
            errors.push(RuleError {
                rule_id: spec.id.clone(),
                field: "options.min_occurrences".into(),
                message: "min_occurrences 必须 ≥ 2".into(),
            });
        }
    }
    if errors.is_empty() {
        Ok(MetricRule {
            spec: clone_spec(spec),
            kind,
            options: spec.options.clone(),
        })
    } else {
        Err(errors)
    }
}

pub enum CompiledRule {
    Ast(AstRule),
    Text(TextRule),
    Metric(MetricRule),
    Taint(TaintRule),
    Inheritance(InheritanceRule),
}

pub fn compile(specs: Vec<RuleSpec>) -> Result<Vec<CompiledRule>, Vec<RuleError>> {
    let mut compiled = Vec::with_capacity(specs.len());
    let mut errors = Vec::new();

    for spec in specs {
        match (
            spec.pattern.as_deref(),
            spec.text.as_deref(),
            spec.metric.as_deref(),
            spec.taint.as_ref(),
            spec.inheritance.as_ref(),
        ) {
            (Some(pattern), None, None, None, None) => match compile_ast(&spec, pattern) {
                Ok(rule) => compiled.push(CompiledRule::Ast(rule)),
                Err(mut errs) => errors.append(&mut errs),
            },
            (None, Some(text), None, None, None) => {
                let regex = Regex::new(text).map_err(|e| RuleError {
                    rule_id: spec.id.clone(),
                    field: "text".into(),
                    message: e.to_string(),
                });
                let exclude = spec
                    .text_exclude
                    .as_deref()
                    .map(Regex::new)
                    .transpose()
                    .map_err(|e| RuleError {
                        rule_id: spec.id.clone(),
                        field: "text_exclude".into(),
                        message: e.to_string(),
                    });
                let mut path_errors = Vec::new();
                let exclude_paths = compile_path_excludes(&spec, &mut path_errors);
                match (regex, exclude) {
                    (Ok(regex), Ok(exclude)) => {
                        if path_errors.is_empty() {
                            compiled.push(CompiledRule::Text(TextRule {
                                spec: clone_spec(&spec),
                                regex,
                                exclude,
                                exclude_paths,
                            }));
                        } else {
                            errors.append(&mut path_errors);
                        }
                    }
                    (Err(error), _) | (_, Err(error)) => errors.push(error),
                }
            }
            (None, None, Some(metric), None, None) => match compile_metric(&spec, metric) {
                Ok(rule) => compiled.push(CompiledRule::Metric(rule)),
                Err(mut errs) => errors.append(&mut errs),
            },
            (None, None, None, Some(taint), None) => match compile_taint(&spec, taint) {
                Ok(rule) => compiled.push(CompiledRule::Taint(rule)),
                Err(mut errs) => errors.append(&mut errs),
            },
            (None, None, None, None, Some(inheritance)) => {
                compiled.push(CompiledRule::Inheritance(InheritanceRule {
                    spec: spec.clone(),
                    target: inheritance.target.clone(),
                    methods: inheritance.methods.clone(),
                    empty_only: inheritance.empty_only,
                    indirect_only: inheritance.indirect_only,
                }));
            }
            _ => {
                // 互斥性已在 rules.rs 校验；防御性兜底。
                errors.push(RuleError {
                    rule_id: spec.id.clone(),
                    field: "pattern|text|metric|taint|inheritance".into(),
                    message: "body 形态不合法".into(),
                });
            }
        }
    }

    if errors.is_empty() {
        Ok(compiled)
    } else {
        Err(errors)
    }
}

fn compile_taint(
    spec: &RuleSpec,
    taint: &crate::rules::TaintSpec,
) -> Result<TaintRule, Vec<RuleError>> {
    let lang = match spec.language {
        RuleLanguage::One(lang) => lang,
        RuleLanguage::All => {
            return Err(vec![RuleError {
                rule_id: spec.id.clone(),
                field: "language".into(),
                message: "污点规则必须指定具体语言".into(),
            }])
        }
    };
    let ts_lang = lang.tree_sitter_language();

    let mut errors = Vec::new();
    let compile_list = |queries: &[String], field: &str, errors: &mut Vec<RuleError>| -> Vec<Query> {
        queries
            .iter()
            .filter_map(|q| match Query::new(&ts_lang, q) {
                Ok(query) if query_has_match(&query) => Some(query),
                Ok(_) => {
                    errors.push(RuleError {
                        rule_id: spec.id.clone(),
                        field: field.into(),
                        message: "taint query 必须包含 @match 捕获".into(),
                    });
                    None
                }
                Err(e) => {
                    errors.push(RuleError {
                        rule_id: spec.id.clone(),
                        field: field.into(),
                        message: format!("tree-sitter query 编译失败：{e}"),
                    });
                    None
                }
            })
            .collect()
    };

    let source_queries = compile_list(&taint.sources, "taint.sources", &mut errors);
    let sink_queries = compile_list(&taint.sinks, "taint.sinks", &mut errors);
    let sanitizer_queries = compile_list(&taint.sanitizers, "taint.sanitizers", &mut errors);
    let mut imports = Vec::new();
    for regex_src in &spec.imports {
        match Regex::new(regex_src) {
            Ok(regex) => imports.push(regex),
            Err(error) => errors.push(RuleError {
                rule_id: spec.id.clone(),
                field: "imports".into(),
                message: error.to_string(),
            }),
        }
    }

    if errors.is_empty() {
        Ok(TaintRule {
            spec: spec.clone(),
            source_queries,
            sink_queries,
            sanitizer_queries,
            imports,
        })
    } else {
        Err(errors)
    }
}

fn clone_spec(spec: &RuleSpec) -> RuleSpec {
    RuleSpec {
        id: spec.id.clone(),
        language: spec.language,
        dimension: spec.dimension,
        severity: spec.severity,
        message: spec.message.clone(),
        pattern: None,
        text: None,
        text_exclude: None,
        metric: None,
        taint: None,
        inheritance: None,
        options: spec.options.clone(),
        filters_not: Vec::new(),
        filters_inside: Vec::new(),
        metavariables: Vec::new(),
        imports: Vec::new(),
        imported_symbols: Vec::new(),
        no_shadow: false,
        field_receiver: false,
        exclude_paths: Vec::new(),
    }
}

fn compile_ast(spec: &RuleSpec, pattern: &str) -> Result<AstRule, Vec<RuleError>> {
    let mut errors = Vec::new();
    let lang = match spec.language {
        RuleLanguage::One(lang) => lang,
        RuleLanguage::All => {
            return Err(vec![RuleError {
                rule_id: spec.id.clone(),
                field: "language".into(),
                message: "AST 规则必须指定具体语言".into(),
            }])
        }
    };
    let ts_lang = lang.tree_sitter_language();

    let query = match Query::new(&ts_lang, pattern) {
        Ok(q) => q,
        Err(e) => {
            return Err(vec![RuleError {
                rule_id: spec.id.clone(),
                field: "pattern".into(),
                message: format!("tree-sitter query 编译失败：{e}"),
            }])
        }
    };
    if !query_has_match(&query) {
        errors.push(RuleError {
            rule_id: spec.id.clone(),
            field: "pattern".into(),
            message: "pattern 必须包含 @match 捕获".into(),
        });
    }

    let mut not_queries = Vec::new();
    for neg in &spec.filters_not {
        match Query::new(&ts_lang, neg) {
            Ok(q) if query_has_match(&q) => not_queries.push(q),
            Ok(_) => errors.push(RuleError {
                rule_id: spec.id.clone(),
                field: "filters.not".into(),
                message: "filter query 必须包含 @match 捕获".into(),
            }),
            Err(e) => errors.push(RuleError {
                rule_id: spec.id.clone(),
                field: "filters.not".into(),
                message: format!("tree-sitter query 编译失败：{e}"),
            }),
        }
    }
    let mut inside_queries = Vec::new();
    for ins in &spec.filters_inside {
        match Query::new(&ts_lang, ins) {
            Ok(q) if query_has_match(&q) => inside_queries.push(q),
            Ok(_) => errors.push(RuleError {
                rule_id: spec.id.clone(),
                field: "filters.inside".into(),
                message: "filter query 必须包含 @match 捕获".into(),
            }),
            Err(e) => errors.push(RuleError {
                rule_id: spec.id.clone(),
                field: "filters.inside".into(),
                message: format!("tree-sitter query 编译失败：{e}"),
            }),
        }
    }

    let capture_names: Vec<String> = query
        .capture_names()
        .iter()
        .map(|n| n.to_string())
        .collect();
    let mut imports = Vec::new();
    for regex_src in &spec.imports {
        match Regex::new(regex_src) {
            Ok(re) => imports.push(re),
            Err(e) => errors.push(RuleError {
                rule_id: spec.id.clone(),
                field: "imports".into(),
                message: e.to_string(),
            }),
        }
    }

    let mut metavariables = Vec::new();
    for (capture, regex_src) in &spec.metavariables {
        if !capture_names.iter().any(|n| n == capture) {
            errors.push(RuleError {
                rule_id: spec.id.clone(),
                field: "metavariables.capture".into(),
                message: format!("捕获 @{capture} 不在 pattern 中声明"),
            });
            continue;
        }
        match Regex::new(regex_src) {
            Ok(re) => metavariables.push((capture.clone(), re)),
            Err(e) => errors.push(RuleError {
                rule_id: spec.id.clone(),
                field: "metavariables.regex".into(),
                message: e.to_string(),
            }),
        }
    }

    if errors.is_empty() {
        Ok(AstRule {
            spec: clone_spec_body(spec),
            query,
            not_queries,
            inside_queries,
            metavariables,
            imports,
            imported_symbols: spec.imported_symbols.clone(),
            no_shadow: spec.no_shadow,
            exclude_paths: compile_path_excludes(spec, &mut errors),
        })
    } else {
        Err(errors)
    }
}

/// 保留原 spec 全字段（AstRule 执行时需要 filters 之外的原样元数据）。
fn clone_spec_body(spec: &RuleSpec) -> RuleSpec {
    spec.clone()
}
