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
}

pub struct TextRule {
    pub spec: RuleSpec,
    pub regex: Regex,
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

pub enum CompiledRule {
    Ast(AstRule),
    Text(TextRule),
    Metric(MetricRule),
    Taint(TaintRule),
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
        ) {
            (Some(pattern), None, None, None) => match compile_ast(&spec, pattern) {
                Ok(rule) => compiled.push(CompiledRule::Ast(rule)),
                Err(mut errs) => errors.append(&mut errs),
            },
            (None, Some(text), None, None) => match Regex::new(text) {
                Ok(regex) => compiled.push(CompiledRule::Text(TextRule {
                    spec: clone_spec(&spec),
                    regex,
                })),
                Err(e) => errors.push(RuleError {
                    rule_id: spec.id.clone(),
                    field: "text".into(),
                    message: e.to_string(),
                }),
            },
            (None, None, Some(metric), None) => match MetricKind::parse(metric) {
                Some(kind) => compiled.push(CompiledRule::Metric(MetricRule {
                    spec: clone_spec(&spec),
                    kind,
                    options: spec.options.clone(),
                })),
                None => errors.push(RuleError {
                    rule_id: spec.id.clone(),
                    field: "metric".into(),
                    message: format!("未知度量 {metric:?}"),
                }),
            },
            (None, None, None, Some(taint)) => match compile_taint(&spec, taint) {
                Ok(rule) => compiled.push(CompiledRule::Taint(rule)),
                Err(mut errs) => errors.append(&mut errs),
            },
            _ => {
                // 互斥性已在 rules.rs 校验；防御性兜底。
                errors.push(RuleError {
                    rule_id: spec.id.clone(),
                    field: "pattern|text|metric|taint".into(),
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
                Ok(query) => Some(query),
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

    if errors.is_empty() {
        Ok(TaintRule {
            spec: spec.clone(),
            source_queries,
            sink_queries,
            sanitizer_queries,
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
        metric: None,
        taint: None,
        options: spec.options.clone(),
        filters_not: Vec::new(),
        filters_inside: Vec::new(),
        metavariables: Vec::new(),
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

    let mut not_queries = Vec::new();
    for neg in &spec.filters_not {
        match Query::new(&ts_lang, neg) {
            Ok(q) => not_queries.push(q),
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
            Ok(q) => inside_queries.push(q),
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
        })
    } else {
        Err(errors)
    }
}

/// 保留原 spec 全字段（AstRule 执行时需要 filters 之外的原样元数据）。
fn clone_spec_body(spec: &RuleSpec) -> RuleSpec {
    spec.clone()
}
