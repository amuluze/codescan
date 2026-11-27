//! 规则资产模型：YAML 解析 + 静态校验（枚举冻结、字段互斥、id 合法性）。
//! 校验范围 = 启用集（Domain §3.3）；任一错误即拒绝，不跳过坏规则。

use crate::model::{Dimension, Severity};
use std::collections::HashSet;

/// 单条规则声明。五种形态互斥：`pattern`（AST query）/ `text`（正则）/
/// `metric` / `taint`（数据流）/ `inheritance`（Java 类型继承）。
#[derive(Debug, Clone, PartialEq)]
pub struct RuleSpec {
    pub id: String,
    pub language: RuleLanguage,
    pub dimension: Dimension,
    pub severity: Severity,
    pub message: String,
    pub pattern: Option<String>,
    pub text: Option<String>,
    /// 文本规则命中文本的排除正则（如 XML 命名空间 URL）。
    pub text_exclude: Option<String>,
    pub metric: Option<String>,
    pub taint: Option<TaintSpec>,
    pub inheritance: Option<InheritanceSpec>,
    pub options: Vec<(String, f64)>,
    pub filters_not: Vec<String>,
    pub filters_inside: Vec<String>,
    pub metavariables: Vec<(String, String)>,
    /// 可选导入条件（正则）：文件至少一条 import 声明匹配任一正则时才应用该规则。
    pub imports: Vec<String>,
    /// 允许通过导入别名满足 metavariable 的原始符号名。
    pub imported_symbols: Vec<String>,
    /// 同名本地定义存在时丢弃该规则的命中（用于 bare 导入规则）。
    pub no_shadow: bool,
    /// Java：接收者名必须匹配同类中的 ObjectInputStream 字段。
    pub field_receiver: bool,
    /// 路径排除正则（应用于 pattern/text 规则）：命中路径的规则不执行。
    pub exclude_paths: Vec<String>,
}

/// 污点端点声明（Domain §4a）：source/sink 必填，sanitizer 可选，均为该语言 query。
#[derive(Debug, Clone, PartialEq)]
pub struct TaintSpec {
    pub sources: Vec<String>,
    pub sinks: Vec<String>,
    pub sanitizers: Vec<String>,
}

/// 类型继承规则（Domain §4c）：在继承闭包包含 target 的类中检查方法。
#[derive(Debug, Clone, PartialEq)]
pub struct InheritanceSpec {
    pub target: String,
    pub methods: Vec<String>,
    pub empty_only: bool,
    /// 仅报告“非直接实现、经继承可达 target”的类；用于与直接实现 AST 规则分工。
    pub indirect_only: bool,
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
#[serde(deny_unknown_fields)]
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
    text_exclude: Option<String>,
    #[serde(default)]
    metric: Option<String>,
    #[serde(default)]
    taint: Option<RawTaint>,
    #[serde(default)]
    inheritance: Option<RawInheritance>,
    #[serde(default)]
    options: std::collections::BTreeMap<String, serde_yaml::Value>,
    #[serde(default)]
    filters: Vec<RawFilter>,
    #[serde(default)]
    metavariables: Vec<RawMetaVar>,
    #[serde(default)]
    imports: Vec<String>,
    #[serde(default)]
    imported_symbols: Vec<String>,
    #[serde(default)]
    no_shadow: bool,
    #[serde(default)]
    field_receiver: bool,
    #[serde(default)]
    exclude_paths: Vec<String>,
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct RawTaint {
    sources: Vec<String>,
    sinks: Vec<String>,
    #[serde(default)]
    sanitizers: Vec<String>,
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct RawInheritance {
    target: String,
    methods: Vec<String>,
    #[serde(default)]
    empty_only: bool,
    #[serde(default)]
    indirect_only: bool,
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct RawFilter {
    not: Option<RawPattern>,
    inside: Option<RawPattern>,
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct RawPattern {
    pattern: String,
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct RawMetaVar {
    capture: String,
    regex: String,
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
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
            raw_rule.inheritance.is_some(),
        ]
        .iter()
        .filter(|b| **b)
        .count();
        if body_kinds != 1 {
            push_error(
                "pattern|text|metric|taint|inheritance",
                "pattern / text / metric / taint / inheritance 必须且只能提供其一".into(),
            );
        }
        if raw_rule.pattern.is_some() && language == RuleLanguage::All {
            push_error("language", "AST 规则不允许 `*`，必须指定具体语言".into());
        }
        if raw_rule.text.is_some() && raw_rule.language != "*" {
            push_error("language", "文本规则 language 必须为 `*`".into());
        }
        if raw_rule.text_exclude.is_some() && raw_rule.text.is_none() {
            push_error("text_exclude", "text_exclude 仅适用于 text 规则".into());
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
        if let Some(inheritance) = &raw_rule.inheritance {
            if language != RuleLanguage::One(crate::language::Language::Java) {
                push_error("language", "继承规则当前仅支持 java".into());
            }
            if inheritance.target.trim().is_empty() {
                push_error("inheritance.target", "必须提供 target 类型".into());
            }
            if inheritance.methods.is_empty() {
                push_error("inheritance.methods", "至少需要一个方法名".into());
            }
            if inheritance
                .methods
                .iter()
                .any(|method| method.trim().is_empty())
            {
                push_error("inheritance.methods", "方法名不能为空".into());
            }
        }
        if raw_rule.pattern.is_none() {
            if !raw_rule.filters.is_empty() {
                push_error("filters", "filters 仅适用于 pattern 规则".into());
            }
            if !raw_rule.metavariables.is_empty() {
                push_error("metavariables", "metavariables 仅适用于 pattern 规则".into());
            }
            if !raw_rule.imports.is_empty() && raw_rule.taint.is_none() {
                push_error("imports", "imports 仅适用于 pattern 或 taint 规则".into());
            }
            if !raw_rule.imported_symbols.is_empty() && raw_rule.taint.is_none() {
                push_error("imported_symbols", "imported_symbols 仅适用于 pattern 或 taint 规则".into());
            }
        }
        if raw_rule.metric.is_none() && !raw_rule.options.is_empty() {
            push_error("options", "options 仅适用于 metric 规则".into());
        }
        if !raw_rule.exclude_paths.is_empty()
            && raw_rule.pattern.is_none()
            && raw_rule.text.is_none()
        {
            push_error("exclude_paths", "exclude_paths 仅适用于 pattern/text 规则".into());
        }

        let mut filters_not = Vec::new();
        let mut filters_inside = Vec::new();
        for filter in &raw_rule.filters {
            if filter.not.is_none() && filter.inside.is_none() {
                push_error("filters", "filters 条目必须提供 not 或 inside".into());
            }
            if let Some(not) = &filter.not {
                filters_not.push(not.pattern.clone());
            }
            if let Some(inside) = &filter.inside {
                filters_inside.push(inside.pattern.clone());
            }
        }

        let mut options = Vec::new();
        for (key, value) in &raw_rule.options {
            match value {
                serde_yaml::Value::Number(number) => match number.as_f64() {
                    Some(number) => options.push((key.clone(), number)),
                    None => push_error("options", format!("option {key} 不是可表示的数值")),
                },
                _ => push_error("options", format!("option {key} 必须是数字")),
            }
        }

        specs.push(RuleSpec {
            id,
            language,
            dimension,
            severity,
            message: raw_rule.message,
            pattern: raw_rule.pattern,
            text: raw_rule.text,
            text_exclude: raw_rule.text_exclude,
            metric: raw_rule.metric,
            taint: raw_rule.taint.map(|t| TaintSpec {
                sources: t.sources,
                sinks: t.sinks,
                sanitizers: t.sanitizers,
            }),
            inheritance: raw_rule.inheritance.map(|i| InheritanceSpec {
                target: i.target,
                methods: i.methods,
                empty_only: i.empty_only,
                indirect_only: i.indirect_only,
            }),
            options,
            filters_not,
            filters_inside,
            metavariables: raw_rule
                .metavariables
                .iter()
                .map(|m| (m.capture.clone(), m.regex.clone()))
                .collect(),
            imports: raw_rule.imports,
            imported_symbols: raw_rule.imported_symbols,
            no_shadow: raw_rule.no_shadow,
            field_receiver: raw_rule.field_receiver,
            exclude_paths: raw_rule.exclude_paths,
        });
    }

    if errors.is_empty() {
        Ok(specs)
    } else {
        Err(errors)
    }
}

/// 逐条解析规则集：单条规则的 YAML/字段错误不会拖垮整份文件。
/// 返回 (有效规则, 错误)；调用方按 disabled 集决定错误是否致命。
pub fn parse_rule_set_lenient(yaml: &str) -> (Vec<RuleSpec>, Vec<RuleError>) {
    let value: serde_yaml::Value = match serde_yaml::from_str(yaml) {
        Ok(value) => value,
        Err(error) => {
            return (
                Vec::new(),
                vec![RuleError {
                    rule_id: "<file>".into(),
                    field: "yaml".into(),
                    message: error.to_string(),
                }],
            )
        }
    };
    let Some(mapping) = value.as_mapping() else {
        return (
            Vec::new(),
            vec![RuleError {
                rule_id: "<file>".into(),
                field: "yaml".into(),
                message: "规则文件根节点必须是 mapping".into(),
            }],
        );
    };
    if mapping
        .keys()
        .any(|key| key.as_str() != Some("rules"))
    {
        return (
            Vec::new(),
            vec![RuleError {
                rule_id: "<file>".into(),
                field: "yaml".into(),
                message: "规则文件只允许顶层字段 rules".into(),
            }],
        );
    }
    let Some(rules_value) = mapping.get(serde_yaml::Value::String("rules".into())) else {
        return (
            Vec::new(),
            vec![RuleError {
                rule_id: "<file>".into(),
                field: "yaml".into(),
                message: "缺少顶层字段 rules".into(),
            }],
        );
    };
    let Some(rules) = rules_value.as_sequence() else {
        return (
            Vec::new(),
            vec![RuleError {
                rule_id: "<file>".into(),
                field: "yaml".into(),
                message: "rules 必须是序列".into(),
            }],
        );
    };

    let mut specs = Vec::new();
    let mut errors = Vec::new();
    for rule_value in rules {
        let rule_id = rule_value
            .as_mapping()
            .and_then(|rule| rule.get(serde_yaml::Value::String("id".into())))
            .and_then(|id| id.as_str())
            .unwrap_or("<unknown>")
            .to_string();
        let mut one = serde_yaml::Mapping::new();
        one.insert(
            serde_yaml::Value::String("rules".into()),
            serde_yaml::Value::Sequence(vec![rule_value.clone()]),
        );
        let document = serde_yaml::Value::Mapping(one);
        let yaml = match serde_yaml::to_string(&document) {
            Ok(yaml) => yaml,
            Err(error) => {
                errors.push(RuleError {
                    rule_id,
                    field: "yaml".into(),
                    message: error.to_string(),
                });
                continue;
            }
        };
        match parse_rule_set(&yaml) {
            Ok(mut parsed) => specs.append(&mut parsed),
            Err(mut rule_errors) => {
                for error in &mut rule_errors {
                    if error.rule_id == "<file>" {
                        error.rule_id = rule_id.clone();
                    }
                }
                errors.append(&mut rule_errors);
            }
        }
    }
    (specs, errors)
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
