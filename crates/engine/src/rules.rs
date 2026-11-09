//! 规则资产模型：YAML 解析 + 静态校验（枚举冻结、字段互斥、id 合法性）。
//! 校验范围 = 启用集（Domain §3.3）；任一错误即拒绝，不跳过坏规则。

use crate::model::{Dimension, Severity};
use std::collections::HashSet;

/// 单条规则声明。三种形态互斥：`pattern`（AST query）/ `text`（正则）/ `metric`。
#[derive(Debug, Clone, PartialEq)]
pub struct RuleSpec {
    pub id: String,
    pub language: RuleLanguage,
    pub dimension: Dimension,
    pub severity: Severity,
    pub message: String,
    pub pattern: Option<String>,
    pub text: Option<String>,
    pub metric: Option<String>,
    pub taint: Option<TaintSpec>,
    pub options: Vec<(String, f64)>,
    pub filters_not: Vec<String>,
    pub filters_inside: Vec<String>,
    pub metavariables: Vec<(String, String)>,
}

/// 污点端点声明（Domain §4a）：source/sink 必填，sanitizer 可选，均为该语言 query。
#[derive(Debug, Clone, PartialEq)]
pub struct TaintSpec {
    pub sources: Vec<String>,
    pub sinks: Vec<String>,
    pub sanitizers: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuleLanguage {
    All,
    One(crate::language::Language),
}

/// 规则错误：带规则 id 的结构化校验失败（Task：逐条列出）。
#[derive(Debug, Clone, PartialEq)]
pub struct RuleError {
    pub rule_id: String,
    pub field: String,
    pub message: String,
}

impl std::fmt::Display for RuleError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "规则[{}] 字段{}：{}", self.rule_id, self.field, self.message)
    }
}

#[derive(serde::Deserialize)]
struct RawRule {
    id: String,
    language: String,
    dimension: String,
    severity: String,
    message: String,
    #[serde(default)]
    pattern: Option<String>,
    #[serde(default)]
    text: Option<String>,
    #[serde(default)]
    metric: Option<String>,
    #[serde(default)]
    taint: Option<RawTaint>,
    #[serde(default)]
    options: std::collections::BTreeMap<String, serde_yaml::Value>,
    #[serde(default)]
    filters: Vec<RawFilter>,
    #[serde(default)]
    metavariables: Vec<RawMetaVar>,
}

#[derive(serde::Deserialize)]
struct RawTaint {
    sources: Vec<String>,
    sinks: Vec<String>,
    #[serde(default)]
    sanitizers: Vec<String>,
}

#[derive(serde::Deserialize)]
struct RawFilter {
    not: Option<RawPattern>,
    inside: Option<RawPattern>,
}

#[derive(serde::Deserialize)]
struct RawPattern {
    pattern: String,
}

#[derive(serde::Deserialize)]
struct RawMetaVar {
    capture: String,
    regex: String,
}

#[derive(serde::Deserialize)]
struct RawRuleSet {
    rules: Vec<RawRule>,
}

/// 解析并静态校验一段规则集 YAML。返回 (specs, errors)；errors 非空时 specs 为空。
pub fn parse_rule_set(yaml: &str) -> Result<Vec<RuleSpec>, Vec<RuleError>> {
    let raw: RawRuleSet = serde_yaml::from_str(yaml).map_err(|e| {
        vec![RuleError {
            rule_id: "<file>".into(),
            field: "yaml".into(),
            message: e.to_string(),
        }]
    })?;

    let mut specs = Vec::new();
    let mut errors = Vec::new();
    let mut seen_ids: HashSet<String> = HashSet::new();

    for raw_rule in raw.rules {
        let id = raw_rule.id.clone();
        let mut push_error = |field: &str, message: String| {
            errors.push(RuleError {
                rule_id: id.clone(),
                field: field.into(),
                message,
            });
        };

        if !seen_ids.insert(id.clone()) {
            push_error("id", format!("规则 id 重复：{id}"));
            continue;
        }
        if id.is_empty()
            || !id
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '/' || c == '-')
        {
            push_error("id", "id 只允许小写字母/数字/`-`/`/`".into());
        }

        let dimension = match raw_rule.dimension.as_str() {
            "security" => Dimension::Security,
            "business-logic" => Dimension::BusinessLogic,
            "quality" => Dimension::Quality,
            other => {
                push_error(
                    "dimension",
                    format!("非法值 {other:?}，允许 security|business-logic|quality"),
                );
                Dimension::Quality
            }
        };
        let severity = match raw_rule.severity.as_str() {
            "error" => Severity::Error,
            "warning" => Severity::Warning,
            "info" => Severity::Info,
            other => {
                push_error(
                    "severity",
                    format!("非法值 {other:?}，允许 error|warning|info"),
                );
                Severity::Info
            }
        };
        let language = if raw_rule.language == "*" {
            RuleLanguage::All
        } else if let Some(lang) = crate::language::Language::from_rule_value(&raw_rule.language)
        {
            RuleLanguage::One(lang)
        } else {
            push_error("language", format!("非法值 {:?}", raw_rule.language));
            RuleLanguage::All
        };

        let body_kinds = [
            raw_rule.pattern.is_some(),
            raw_rule.text.is_some(),
            raw_rule.metric.is_some(),
            raw_rule.taint.is_some(),
        ]
        .iter()
        .filter(|b| **b)
        .count();
        if body_kinds != 1 {
            push_error(
                "pattern|text|metric|taint",
                "pattern / text / metric / taint 必须且只能提供其一".into(),
            );
        }
        if raw_rule.pattern.is_some() && language == RuleLanguage::All {
            push_error("language", "AST 规则不允许 `*`，必须指定具体语言".into());
        }
        if raw_rule.text.is_some() && raw_rule.language != "*" {
            push_error("language", "文本规则 language 必须为 `*`".into());
        }
        if raw_rule.metric.is_some() && raw_rule.language != "*" {
            push_error("language", "度量规则 language 必须为 `*`".into());
        }
        if let Some(taint) = &raw_rule.taint {
            if language == RuleLanguage::All {
                push_error("language", "污点规则不允许 `*`，必须指定具体语言".into());
            }
            if taint.sources.is_empty() {
                push_error("taint.sources", "污点规则至少需要一个 source".into());
            }
            if taint.sinks.is_empty() {
                push_error("taint.sinks", "污点规则至少需要一个 sink".into());
            }
        }

        let mut filters_not = Vec::new();
        let mut filters_inside = Vec::new();
        for filter in &raw_rule.filters {
            if let Some(not) = &filter.not {
                filters_not.push(not.pattern.clone());
            }
            if let Some(inside) = &filter.inside {
                filters_inside.push(inside.pattern.clone());
            }
        }

        let options = raw_rule
            .options
            .iter()
            .filter_map(|(k, v)| match v {
                serde_yaml::Value::Number(n) => Some((k.clone(), n.as_f64().unwrap_or(0.0))),
                _ => None,
            })
            .collect();

        specs.push(RuleSpec {
            id,
            language,
            dimension,
            severity,
            message: raw_rule.message,
            pattern: raw_rule.pattern,
            text: raw_rule.text,
            metric: raw_rule.metric,
            taint: raw_rule.taint.map(|t| TaintSpec {
                sources: t.sources,
                sinks: t.sinks,
                sanitizers: t.sanitizers,
            }),
            options,
            filters_not,
            filters_inside,
            metavariables: raw_rule
                .metavariables
                .iter()
                .map(|m| (m.capture.clone(), m.regex.clone()))
                .collect(),
        });
    }

    if errors.is_empty() {
        Ok(specs)
    } else {
        Err(errors)
    }
}

/// 合并内置与自定义规则集：id 冲突即整体拒绝（Domain §4.4）。
pub fn merge_rule_sets(
    builtin: Vec<RuleSpec>,
    custom: Vec<RuleSpec>,
) -> Result<Vec<RuleSpec>, Vec<RuleError>> {
    let mut ids: HashSet<String> = HashSet::new();
    let mut conflicts = Vec::new();
    for spec in builtin.iter().chain(custom.iter()) {
        if !ids.insert(spec.id.clone()) {
            conflicts.push(RuleError {
                rule_id: spec.id.clone(),
                field: "id".into(),
                message: "自定义规则与内置规则 id 冲突".into(),
            });
        }
    }
    if conflicts.is_empty() {
        let mut all = builtin;
        all.extend(custom);
        Ok(all)
    } else {
        Err(conflicts)
    }
}
