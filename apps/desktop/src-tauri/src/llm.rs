//! LLM 辅助层（v3，Domain §3a）：Provider 配置、请求构造与响应解析、
//! 分诊与候选规则提示词、自定义规则合并校验。
//!
//! 纪律：本模块产出永不进入发现/基线/门禁；只有经校验并显式保存的
//! 规则文本才影响后续扫描。请求构造与解析均为纯函数，网络调用单列。

use codescan_engine::Engine;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

pub const SECRET_API_KEY: &str = "llm:api_key";
pub const META_PROVIDER: &str = "llm:provider";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProviderKind {
    OpenAi,
    Anthropic,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProviderConfig {
    pub kind: ProviderKind,
    pub base_url: String,
    pub model: String,
}

impl Default for ProviderConfig {
    fn default() -> Self {
        ProviderConfig {
            kind: ProviderKind::OpenAi,
            base_url: "https://api.openai.com/v1".into(),
            model: String::new(),
        }
    }
}

impl ProviderConfig {
    pub fn validate(&self) -> Result<(), String> {
        if !(self.base_url.starts_with("http://") || self.base_url.starts_with("https://")) {
            return Err("base_url 必须以 http:// 或 https:// 开头".into());
        }
        if self.model.trim().is_empty() {
            return Err("model 不能为空".into());
        }
        Ok(())
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }

    pub fn from_json(json: &str) -> Result<ProviderConfig, String> {
        serde_json::from_str(json).map_err(|e| format!("Provider 配置解析失败：{e}"))
    }
}

/// 分诊结果：展示层参考，不参与门禁（Domain §5.3）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TriageResult {
    /// true-positive | likely-false-positive | uncertain
    pub assessment: String,
    pub rationale: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CandidateRule {
    pub yaml: String,
    pub valid: bool,
    pub errors: Vec<String>,
}

/// 发现与规则上下文（构建提示词）。
#[derive(Debug, Clone)]
pub struct FindingContext {
    pub rule_id: String,
    pub language: String,
    pub dimension: String,
    pub severity: String,
    pub message: String,
    pub path: String,
    pub line: usize,
    pub match_text: String,
    /// 既有规则的可读体（query/正则/度量）。
    pub rule_source: String,
}

fn truncate(text: &str, max: usize) -> String {
    if text.len() <= max {
        text.to_string()
    } else {
        let mut end = max;
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        format!("{}…", &text[..end])
    }
}

// ---------- 请求构造（纯函数，单测覆盖） ----------

pub fn openai_body(model: &str, system: &str, user: &str) -> serde_json::Value {
    serde_json::json!({
        "model": model,
        "messages": [
            { "role": "system", "content": system },
            { "role": "user", "content": user },
        ],
        "temperature": 0,
    })
}

pub fn anthropic_body(model: &str, system: &str, user: &str) -> serde_json::Value {
    serde_json::json!({
        "model": model,
        "max_tokens": 4096,
        "system": system,
        "messages": [
            { "role": "user", "content": user },
        ],
        "temperature": 0,
    })
}

pub fn endpoint(config: &ProviderConfig) -> String {
    let base = config.base_url.trim_end_matches('/');
    match config.kind {
        ProviderKind::OpenAi => format!("{base}/chat/completions"),
        ProviderKind::Anthropic => format!("{base}/v1/messages"),
    }
}

pub fn parse_openai_response(body: &str) -> Result<String, String> {
    let value: serde_json::Value =
        serde_json::from_str(body).map_err(|e| format!("响应不是 JSON：{e}"))?;
    value["choices"][0]["message"]["content"]
        .as_str()
        .map(|s| s.to_string())
        .ok_or_else(|| {
            format!(
                "响应缺少 choices[0].message.content：{}",
                truncate(body, 300)
            )
        })
}

pub fn parse_anthropic_response(body: &str) -> Result<String, String> {
    let value: serde_json::Value =
        serde_json::from_str(body).map_err(|e| format!("响应不是 JSON：{e}"))?;
    value["content"][0]["text"]
        .as_str()
        .map(|s| s.to_string())
        .ok_or_else(|| {
            format!(
                "响应缺少 content[0].text：{}",
                truncate(body, 300)
            )
        })
}

// ---------- 提示词与模型输出解析 ----------

fn triage_system_prompt() -> &'static str {
    "你是代码扫描结果的误报分诊助手。只输出一个 JSON 对象，不要 markdown 代码块，不要在 JSON 外输出任何文字。\
JSON 结构：{\"assessment\":\"true-positive|likely-false-positive|uncertain\",\"rationale\":\"不超过三句中文理由\"}。\
你只能基于给出的证据判断，不得声称已验证可利用性。"
}

pub fn triage_prompt(finding: &FindingContext) -> (String, String) {
    let user = format!(
        "扫描发现：\n规则: {rule_id}（{language} / {dimension} / {severity}）\n规则说明: {message}\n规则体:\n{rule_source}\n位置: {path}:{line}\n命中文本（截断）:\n{match}\n\n请判断该命中更可能是真实问题还是误报。",
        rule_id = finding.rule_id,
        language = finding.language,
        dimension = finding.dimension,
        severity = finding.severity,
        message = finding.message,
        rule_source = truncate(&finding.rule_source, 1500),
        path = finding.path,
        line = finding.line,
        match = truncate(&finding.match_text, 4000),
    );
    (triage_system_prompt().to_string(), user)
}

pub fn parse_triage(text: &str) -> Result<TriageResult, String> {
    let json_text = extract_json_object(text)
        .ok_or_else(|| format!("模型输出中没有 JSON 对象：{}", truncate(text, 300)))?;
    let value: serde_json::Value = serde_json::from_str(&json_text)
        .map_err(|e| format!("分诊 JSON 解析失败：{e}"))?;
    let assessment = value["assessment"].as_str().unwrap_or("").to_string();
    if !matches!(
        assessment.as_str(),
        "true-positive" | "likely-false-positive" | "uncertain"
    ) {
        return Err(format!("非法的 assessment：{assessment:?}"));
    }
    let rationale = value["rationale"].as_str().unwrap_or("").to_string();
    Ok(TriageResult {
        assessment,
        rationale,
    })
}

fn extract_json_object(text: &str) -> Option<String> {
    let start = text.find('{')?;
    let end = text.rfind('}')?;
    (end > start).then(|| text[start..=end].to_string())
}

fn candidate_system_prompt() -> &'static str {
    "你是 Codescan 静态规则的编写助手。只输出一个 YAML 文档，不要 markdown 代码块，不要注释以外的解释文字。\
格式必须是：rules: 下恰好一条规则。字段：id（必须以 custom/ 开头，小写字母数字/`-`/`/`）、language（typescript|tsx|javascript|jsx|python|java|go|rust）、\
dimension（security|business-logic|quality）、severity（error|warning|info）、message（只描述出现了什么构造，不得声称漏洞可利用）、\
以及以下二者之一：pattern（tree-sitter query，必须包含 @match 捕获）或 text（Rust regex，language 必须为 \"*\"）。\
pattern 的节点名必须真实存在于目标语言的 tree-sitter 语法；不确定时优先写更窄、更精确的匹配。不要使用 filters 与 taint。"
}

pub fn candidate_prompt(finding: &FindingContext) -> (String, String) {
    let user = format!(
        "请为下面这个扫描发现生成一条把同类问题泛化的自定义规则。\n\
语言: {language}\n发现规则: {rule_id}（{dimension}/{severity}）\n规则说明: {message}\n\
位置: {path}:{line}\n命中文本（截断）:\n{match}\n\n参考现有规则体（可借鉴匹配方式，但新规则 id 必须用 custom/ 前缀）:\n{rule_source}",
        language = finding.language,
        rule_id = finding.rule_id,
        dimension = finding.dimension,
        severity = finding.severity,
        message = finding.message,
        path = finding.path,
        line = finding.line,
        match = truncate(&finding.match_text, 4000),
        rule_source = truncate(&finding.rule_source, 1200),
    );
    (candidate_system_prompt().to_string(), user)
}

/// 从模型输出中提取规则 YAML（剥代码围栏、截取 rules: 文档）。
pub fn extract_rule_yaml(text: &str) -> Result<String, String> {
    let mut body = text.trim().to_string();
    if let Some(fence_start) = body.find("```") {
        let after = &body[fence_start + 3..];
        let after = after.strip_prefix("yaml").unwrap_or(after);
        let after = after.strip_prefix('\n').unwrap_or(after);
        if let Some(fence_end) = after.find("```") {
            body = after[..fence_end].trim().to_string();
        }
    }
    match body.find("rules:") {
        Some(idx) => Ok(body[idx..].trim().to_string()),
        None => Err(format!(
            "模型输出中没有 rules: 文档：{}",
            truncate(text, 300)
        )),
    }
}

// ---------- 自定义规则合并与校验 ----------

/// 校验候选规则文本：可解析、可编译、id 全部 `custom/` 前缀。
pub fn validate_candidate(yaml_text: &str) -> Vec<String> {
    let mut errors = Vec::new();
    let specs = match codescan_engine::rules::parse_rule_set(yaml_text) {
        Ok(specs) => specs,
        Err(errs) => {
            return errs.iter().map(|e| e.to_string()).collect();
        }
    };
    for spec in &specs {
        if !spec.id.starts_with("custom/") {
            errors.push(format!("规则 id {} 必须以 custom/ 开头", spec.id));
        }
    }
    if !errors.is_empty() {
        return errors;
    }
    match Engine::with_rules(Some(yaml_text), &HashSet::new()) {
        Ok(_) => Vec::new(),
        Err(errs) => errs.iter().map(|e| e.to_string()).collect(),
    }
}

/// 把候选规则合并进既有自定义规则文件内容（保留既有规则原文）。
/// 冲突/非法 → Err，不产生新内容。
pub fn merge_custom_rules(existing: Option<&str>, candidate_yaml: &str) -> Result<String, Vec<String>> {
    let mut errors = validate_candidate(candidate_yaml);
    if !errors.is_empty() {
        return Err(errors);
    }

    let existing_value: serde_yaml::Value = match existing {
        Some(text) if !text.trim().is_empty() => match serde_yaml::from_str(text) {
            Ok(v) => v,
            Err(e) => return Err(vec![format!("既有自定义规则文件解析失败：{e}")]),
        },
        _ => serde_yaml::Value::Mapping(Default::default()),
    };
    let candidate_value: serde_yaml::Value = match serde_yaml::from_str(candidate_yaml) {
        Ok(v) => v,
        Err(e) => return Err(vec![format!("候选规则解析失败：{e}")]),
    };

    let mut existing_rules = existing_value
        .get("rules")
        .and_then(|v| v.as_sequence())
        .cloned()
        .unwrap_or_default();
    let candidate_rules = candidate_value
        .get("rules")
        .and_then(|v| v.as_sequence())
        .cloned()
        .unwrap_or_default();

    // id 冲突检查（既有 rules 中的 id 文本）。
    let existing_ids: Vec<String> = existing_rules
        .iter()
        .filter_map(|r| r.get("id").and_then(|v| v.as_str()).map(|s| s.to_string()))
        .collect();
    for rule in &candidate_rules {
        if let Some(id) = rule.get("id").and_then(|v| v.as_str()) {
            if existing_ids.iter().any(|e| e == id) {
                errors.push(format!("规则 id {id} 已存在于自定义规则中"));
            }
        }
    }
    if !errors.is_empty() {
        return Err(errors);
    }

    existing_rules.extend(candidate_rules);
    let mut merged = serde_yaml::Mapping::new();
    merged.insert(
        serde_yaml::Value::String("rules".into()),
        serde_yaml::Value::Sequence(existing_rules),
    );
    let merged_text = serde_yaml::to_string(&serde_yaml::Value::Mapping(merged))
        .map_err(|e| vec![format!("规则文件序列化失败：{e}")])?;

    // 合并后再做一次整体编译校验（含内置冲突）。
    match Engine::with_rules(Some(&merged_text), &HashSet::new()) {
        Ok(_) => Ok(merged_text),
        Err(errs) => Err(errs.iter().map(|e| e.to_string()).collect()),
    }
}

// ---------- 网络调用（唯一非纯函数入口） ----------

/// 调用 Provider 完成一次补全。错误信息不得包含 API Key。
pub async fn complete(
    config: &ProviderConfig,
    api_key: &str,
    system: &str,
    user: &str,
) -> Result<String, String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(60))
        .build()
        .map_err(|e| format!("HTTP 客户端创建失败：{e}"))?;
    let url = endpoint(config);
    let request = match config.kind {
        ProviderKind::OpenAi => client
            .post(&url)
            .bearer_auth(api_key)
            .json(&openai_body(&config.model, system, user)),
        ProviderKind::Anthropic => client
            .post(&url)
            .header("x-api-key", api_key)
            .header("anthropic-version", "2023-06-01")
            .json(&anthropic_body(&config.model, system, user)),
    };
    let response = request.send().await.map_err(|e| format!("请求失败：{e}"))?;
    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|e| format!("读取响应失败：{e}"))?;
    if !status.is_success() {
        return Err(format!("Provider 返回 {status}：{}", truncate(&body, 500)));
    }
    match config.kind {
        ProviderKind::OpenAi => parse_openai_response(&body),
        ProviderKind::Anthropic => parse_anthropic_response(&body),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn openai_and_anthropic_bodies_and_parsers() {
        let body = openai_body("m1", "sys", "usr");
        assert_eq!(body["model"], "m1");
        assert_eq!(body["messages"][0]["role"], "system");
        let openai_reply = r#"{"choices":[{"message":{"content":"hello"}}]}"#;
        assert_eq!(parse_openai_response(openai_reply).unwrap(), "hello");

        let body = anthropic_body("m2", "sys", "usr");
        assert_eq!(body["max_tokens"], 4096);
        assert_eq!(body["system"], "sys");
        let anthropic_reply = r#"{"content":[{"type":"text","text":"world"}]}"#;
        assert_eq!(parse_anthropic_response(anthropic_reply).unwrap(), "world");
        assert!(parse_anthropic_response(r#"{"error":{"message":"bad"}}"#).is_err());
    }

    #[test]
    fn endpoints_built_from_base() {
        let config = ProviderConfig {
            kind: ProviderKind::OpenAi,
            base_url: "https://x/v1/".into(),
            model: "m".into(),
        };
        assert_eq!(endpoint(&config), "https://x/v1/chat/completions");
        let config = ProviderConfig {
            kind: ProviderKind::Anthropic,
            base_url: "https://y".into(),
            model: "m".into(),
        };
        assert_eq!(endpoint(&config), "https://y/v1/messages");
    }

    #[test]
    fn triage_parses_json_with_fences() {
        let text = "```json\n{\"assessment\":\"uncertain\",\"rationale\":\"证据不足\"}\n```";
        let out = parse_triage(text).unwrap();
        assert_eq!(out.assessment, "uncertain");
        assert_eq!(out.rationale, "证据不足");
        assert!(parse_triage("nope").is_err());
        assert!(parse_triage(r#"{"assessment":"bogus","rationale":"x"}"#).is_err());
    }

    #[test]
    fn candidate_yaml_extraction_and_validation() {
        let text = "Here:\n```yaml\nrules:\n  - id: custom/foo\n    language: typescript\n    dimension: quality\n    severity: info\n    message: m\n    pattern: |\n      (return_statement) @match\n```";
        let yaml = extract_rule_yaml(text).unwrap();
        assert!(validate_candidate(&yaml).is_empty(), "{:?}", validate_candidate(&yaml));

        // 非 custom/ 前缀拒绝。
        let bad = "rules:\n  - id: ts/foo\n    language: typescript\n    dimension: quality\n    severity: info\n    message: m\n    pattern: |\n      (return_statement) @match\n";
        assert!(!validate_candidate(bad).is_empty());
    }

    #[test]
    fn merge_preserves_existing_and_rejects_conflicts() {
        let existing = "rules:\n  - id: custom/a\n    language: typescript\n    dimension: quality\n    severity: info\n    message: m\n    pattern: |\n      (return_statement) @match\n";
        let candidate = "rules:\n  - id: custom/b\n    language: python\n    dimension: quality\n    severity: info\n    message: m\n    pattern: |\n      (return_statement) @match\n";
        let merged = merge_custom_rules(Some(existing), candidate).unwrap();
        assert!(merged.contains("custom/a"), "{merged}");
        assert!(merged.contains("custom/b"), "{merged}");

        let conflict = "rules:\n  - id: custom/a\n    language: python\n    dimension: quality\n    severity: info\n    message: m\n    pattern: |\n      (return_statement) @match\n";
        assert!(merge_custom_rules(Some(existing), conflict).is_err());
    }
}
