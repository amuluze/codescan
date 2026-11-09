//! LLM 辅助层（v3，Domain §3a）：Provider 配置、请求构造与响应解析、
//! 分诊与候选规则提示词、自定义规则合并校验。
//!
//! 纪律：本模块产出永不进入发现/基线/门禁；只有经校验并显式保存的
//! 规则文本才影响后续扫描。请求构造与解析均为纯函数，网络调用单列。

use codescan_engine::model::{Finding, Severity};
use codescan_engine::Engine;
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

pub const SECRET_API_KEY: &str = "llm:api_key";
pub const META_PROVIDER: &str = "llm:provider";
/// 批量分诊单次上限（任务 Spec §2）。
pub const BATCH_TRIAGE_LIMIT: usize = 20;

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

/// 批量分诊单条结果：展示层参考，不参与门禁（Domain §5.3）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BatchTriageEntry {
    pub fingerprint: String,
    /// 有效枚举，或模型缺失该条时的 `uncertain` 占位。
    pub assessment: String,
    pub rationale: String,
    /// 模型未返回的请求指纹，或模型返回了未请求的指纹。
    pub missing: bool,
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
    // content 是块数组：thinking 块只有 `thinking`，文本块才有 `text`。
    // 跳过非文本块并拼接全部文本块，避免扩展思考导致 content[0] 不是 text。
    let mut text = String::new();
    if let Some(blocks) = value["content"].as_array() {
        for block in blocks {
            if let Some(part) = block["text"].as_str() {
                text.push_str(part);
            }
        }
    }
    if text.is_empty() {
        let reason = if value["content"].is_array() {
            "响应没有 text 内容块"
        } else {
            "响应缺少 content 数组"
        };
        return Err(format!("{reason}：{}", truncate(body, 300)));
    }
    Ok(text)
}

// ---------- 流式（SSE）解析：纯函数，单测覆盖 ----------

/// 流式请求体：在非流式请求体上追加 `stream: true`。
pub fn stream_body(config: &ProviderConfig, system: &str, user: &str) -> serde_json::Value {
    let mut body = match config.kind {
        ProviderKind::OpenAi => openai_body(&config.model, system, user),
        ProviderKind::Anthropic => anthropic_body(&config.model, system, user),
    };
    body["stream"] = serde_json::Value::Bool(true);
    body
}

/// 解析一行 SSE：容忍空行、注释、`event:` 行与非 JSON 数据。
/// OpenAI 取 `choices[0].delta.content`；Anthropic 仅取
/// `content_block_delta.delta.text`。空增量与 `[DONE]` 返回 `None`。
pub fn parse_sse_data_line(kind: ProviderKind, line: &str) -> Option<String> {
    let line = line.trim();
    if line.is_empty() || line.starts_with(':') || line.starts_with("event:") {
        return None;
    }
    let payload = line.strip_prefix("data:")?.trim();
    if payload == "[DONE]" {
        return None;
    }
    let value: serde_json::Value = serde_json::from_str(payload).ok()?;
    let text = match kind {
        ProviderKind::OpenAi => value["choices"][0]["delta"]["content"].as_str()?,
        ProviderKind::Anthropic => {
            if value["type"].as_str() != Some("content_block_delta") {
                return None;
            }
            value["delta"]["text"].as_str()?
        }
    };
    (!text.is_empty()).then(|| text.to_string())
}

/// 增量 SSE 解析器：跨网络 chunk 缓冲未完整的行，按字节拼接后解码，
/// 避免多字节 UTF-8 被 chunk 边界切断。
#[derive(Debug)]
pub struct SseParser {
    kind: ProviderKind,
    pending: Vec<u8>,
}

impl SseParser {
    pub fn new(kind: ProviderKind) -> Self {
        SseParser {
            kind,
            pending: Vec::new(),
        }
    }

    pub fn push(&mut self, chunk: &str) -> Vec<String> {
        self.push_bytes(chunk.as_bytes())
    }

    pub fn push_bytes(&mut self, chunk: &[u8]) -> Vec<String> {
        self.pending.extend_from_slice(chunk);
        let mut deltas = Vec::new();
        while let Some(newline) = self.pending.iter().position(|b| *b == b'\n') {
            let line: Vec<u8> = self.pending.drain(..=newline).collect();
            let text = String::from_utf8_lossy(&line);
            if let Some(delta) = parse_sse_data_line(self.kind, &text) {
                deltas.push(delta);
            }
        }
        deltas
    }

    /// 流结束后处理没有换行结尾的残留行。
    pub fn finish(&mut self) -> Vec<String> {
        if self.pending.is_empty() {
            return Vec::new();
        }
        let rest = std::mem::take(&mut self.pending);
        let text = String::from_utf8_lossy(&rest);
        parse_sse_data_line(self.kind, &text)
            .into_iter()
            .collect()
    }
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

// ---------- 批量分诊 ----------

fn severity_rank(severity: Severity) -> u8 {
    match severity {
        Severity::Error => 0,
        Severity::Warning => 1,
        Severity::Info => 2,
    }
}

/// 选择批量分诊目标：显式指纹按传入顺序去重截断；空则按
/// severity error>warning>info、路径、位置、规则 id 确定性排序取前 `limit` 条。
pub fn select_batch_fingerprints(
    findings: &[Finding],
    requested: &[String],
    limit: usize,
) -> Vec<String> {
    if !requested.is_empty() {
        let mut seen = HashSet::new();
        return requested
            .iter()
            .filter(|fp| seen.insert((*fp).clone()))
            .take(limit)
            .cloned()
            .collect();
    }
    let mut ordered: Vec<&Finding> = findings.iter().collect();
    ordered.sort_by(|a, b| {
        severity_rank(a.severity)
            .cmp(&severity_rank(b.severity))
            .then_with(|| a.path.cmp(&b.path))
            .then_with(|| a.span.start.line.cmp(&b.span.start.line))
            .then_with(|| a.span.start.column.cmp(&b.span.start.column))
            .then_with(|| a.rule_id.cmp(&b.rule_id))
            .then_with(|| a.fingerprint.cmp(&b.fingerprint))
    });
    ordered
        .into_iter()
        .take(limit)
        .map(|f| f.fingerprint.clone())
        .collect()
}

fn batch_triage_system_prompt() -> &'static str {
    "你是代码扫描结果的批量误报分诊助手。只输出一个 JSON 数组，不要 markdown 代码块，不要在 JSON 外输出任何文字。\
数组每项结构：{\"fingerprint\":\"输入中的指纹\",\"assessment\":\"true-positive|likely-false-positive|uncertain\",\"rationale\":\"不超过两句中文理由\"}。\
必须为输入中的每个 fingerprint 返回恰好一项，不得返回输入之外的 fingerprint。\
你只能基于给出的证据判断，不得声称已验证可利用性。"
}

pub fn batch_triage_prompt(findings: &[Finding]) -> (String, String) {
    let mut lines = String::new();
    for (idx, finding) in findings.iter().enumerate() {
        lines.push_str(&format!(
            "{n}. fingerprint: {fp}\n规则: {rule}（{dim}/{sev}）\n位置: {path}:{line}\n命中文本（截断）:\n{match}\n\n",
            n = idx + 1,
            fp = finding.fingerprint,
            rule = finding.rule_id,
            dim = serde_json::to_string(&finding.dimension).unwrap_or_default().trim_matches('"'),
            sev = serde_json::to_string(&finding.severity).unwrap_or_default().trim_matches('"'),
            path = finding.path,
            line = finding.span.start.line,
            match = truncate(&finding.match_text, 800),
        ));
    }
    let user = format!(
        "请逐条判断下列 {count} 条扫描发现更可能是真实问题还是误报，并按系统要求输出 JSON 数组。\n\n{lines}",
        count = findings.len(),
    );
    (batch_triage_system_prompt().to_string(), user)
}

fn extract_json_array(text: &str) -> Option<String> {
    let start = text.find('[')?;
    let end = text.rfind(']')?;
    (end > start).then(|| text[start..=end].to_string())
}

/// 解析批量分诊 JSON 数组：校验 assessment 枚举；缺失的请求指纹与模型
/// 多返回的未知指纹都以 `missing: true` 标注，不丢弃任何一侧。
pub fn parse_batch_triage(
    text: &str,
    expected: &[String],
) -> Result<Vec<BatchTriageEntry>, String> {
    let json_text = extract_json_array(text)
        .ok_or_else(|| format!("模型输出中没有 JSON 数组：{}", truncate(text, 300)))?;
    let value: serde_json::Value = serde_json::from_str(&json_text)
        .map_err(|e| format!("批量分诊 JSON 解析失败：{e}"))?;
    let items = value
        .as_array()
        .ok_or_else(|| "批量分诊输出必须是 JSON 数组".to_string())?;

    let mut returned: Vec<BatchTriageEntry> = Vec::new();
    let mut seen = HashSet::new();
    for item in items {
        let fingerprint = item["fingerprint"]
            .as_str()
            .unwrap_or("")
            .trim()
            .to_string();
        if fingerprint.is_empty() {
            return Err("批量分诊条目缺少 fingerprint".to_string());
        }
        let assessment = item["assessment"].as_str().unwrap_or("").to_string();
        if !matches!(
            assessment.as_str(),
            "true-positive" | "likely-false-positive" | "uncertain"
        ) {
            return Err(format!(
                "非法的 assessment：{assessment:?}（fingerprint {fingerprint}）"
            ));
        }
        if !seen.insert(fingerprint.clone()) {
            continue;
        }
        returned.push(BatchTriageEntry {
            fingerprint,
            assessment,
            rationale: item["rationale"].as_str().unwrap_or("").to_string(),
            missing: false,
        });
    }

    let expected_set: HashSet<&str> = expected.iter().map(|s| s.as_str()).collect();
    let mut result = Vec::new();
    for fingerprint in expected {
        match returned.iter().position(|e| &e.fingerprint == fingerprint) {
            Some(pos) => {
                let mut entry = returned.remove(pos);
                entry.missing = false;
                result.push(entry);
            }
            None => result.push(BatchTriageEntry {
                fingerprint: fingerprint.clone(),
                assessment: "uncertain".to_string(),
                rationale: "模型未返回该发现的分诊结果。".to_string(),
                missing: true,
            }),
        }
    }
    for mut entry in returned {
        entry.missing = !expected_set.contains(entry.fingerprint.as_str());
        result.push(entry);
    }
    Ok(result)
}

fn candidate_system_prompt() -> &'static str {
    "你是 Codescan 静态规则的编写助手。只输出一个 YAML 文档，不要 markdown 代码块，不要注释以外的解释文字。\
格式必须是：rules: 下恰好一条规则。字段：id（必须以 custom/ 开头，小写字母数字/`-`/`/`）、language（typescript|tsx|javascript|jsx|python|java|go|rust）、\
dimension（security|business-logic|quality）、severity（error|warning|info）、message（只描述出现了什么构造，不得声称漏洞可利用）、\
以及以下三者之一：pattern（tree-sitter query，必须包含 @match 捕获）、text（Rust regex，language 必须为 \"*\"）或 taint 块。\
taint 块结构：taint: { sources: [query...], sinks: [query...], sanitizers: [query...] }；language 必须为具体语言（不得为 \"*\"）、\
sources 与 sinks 至少各一条，每个 query 必须包含 @match 捕获（sink/source 的 @match 应落在标识符、调用或表达式节点上）。\
taint 表示「不可信数据可能到达 sink」的同文件过程内近似，不得声称跨文件、堆别名或字段敏感证明。\
pattern 的节点名必须真实存在于目标语言的 tree-sitter 语法；不确定时优先写更窄、更精确的匹配。不要使用 filters。"
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

/// 判断 query 是否声明了 `@match` 捕获（避免把 `@match_foo` 误判为合法）。
fn has_match_capture(query: &str) -> bool {
    let mut rest = query;
    while let Some(pos) = rest.find("@match") {
        let after = &rest[pos + "@match".len()..];
        let boundary = after
            .chars()
            .next()
            .map(|c| !(c.is_alphanumeric() || c == '_'))
            .unwrap_or(true);
        if boundary {
            return true;
        }
        rest = after;
    }
    false
}

/// 校验候选规则文本：可解析、可编译、id 全部 `custom/` 前缀；
/// taint 端点的每条 query 必须声明 `@match` 捕获。
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
        if let Some(taint) = &spec.taint {
            let groups = [
                ("taint.sources", &taint.sources),
                ("taint.sinks", &taint.sinks),
                ("taint.sanitizers", &taint.sanitizers),
            ];
            for (field, queries) in groups {
                for (idx, query) in queries.iter().enumerate() {
                    if !has_match_capture(query) {
                        errors.push(format!(
                            "规则 id {} 的 {} 第 {} 条 query 必须包含 @match 捕获",
                            spec.id,
                            field,
                            idx + 1
                        ));
                    }
                }
            }
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

/// 连通性检测：最小请求、短超时；只要 2xx 且返回 JSON 即视为可用。
/// 错误信息不得包含 API Key。
pub async fn probe(config: &ProviderConfig, api_key: &str) -> Result<(), String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|e| format!("HTTP 客户端创建失败：{e}"))?;
    let url = endpoint(config);
    let (body, request) = match config.kind {
        ProviderKind::OpenAi => (
            serde_json::json!({
                "model": config.model,
                "messages": [{ "role": "user", "content": "ping" }],
                "max_tokens": 8,
                "temperature": 0,
            }),
            client.post(&url).bearer_auth(api_key),
        ),
        ProviderKind::Anthropic => (
            serde_json::json!({
                "model": config.model,
                "max_tokens": 8,
                "messages": [{ "role": "user", "content": "ping" }],
                "temperature": 0,
            }),
            client
                .post(&url)
                .header("x-api-key", api_key)
                .header("anthropic-version", "2023-06-01"),
        ),
    };
    let response = request
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("请求失败：{e}"))?;
    let status = response.status();
    let text = response
        .text()
        .await
        .map_err(|e| format!("读取响应失败：{e}"))?;
    if !status.is_success() {
        return Err(format!("Provider 返回 {status}：{}", truncate(&text, 500)));
    }
    let value: serde_json::Value =
        serde_json::from_str(&text).map_err(|e| format!("响应不是 JSON：{e}"))?;
    if !value.is_object() {
        return Err("Provider 响应不是 JSON 对象".into());
    }
    Ok(())
}

/// 流式调用 Provider，每片增量经 `on_delta` 回调，返回完整文本。
/// 错误信息不得包含 API Key。
pub async fn complete_stream<F>(
    config: &ProviderConfig,
    api_key: &str,
    system: &str,
    user: &str,
    mut on_delta: F,
) -> Result<String, String>
where
    F: FnMut(&str),
{
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .build()
        .map_err(|e| format!("HTTP 客户端创建失败：{e}"))?;
    let url = endpoint(config);
    let body = stream_body(config, system, user);
    let request = match config.kind {
        ProviderKind::OpenAi => client.post(&url).bearer_auth(api_key).json(&body),
        ProviderKind::Anthropic => client
            .post(&url)
            .header("x-api-key", api_key)
            .header("anthropic-version", "2023-06-01")
            .json(&body),
    };
    let response = request.send().await.map_err(|e| format!("请求失败：{e}"))?;
    let status = response.status();
    if !status.is_success() {
        let body = response
            .text()
            .await
            .map_err(|e| format!("读取响应失败：{e}"))?;
        return Err(format!("Provider 返回 {status}：{}", truncate(&body, 500)));
    }

    let mut parser = SseParser::new(config.kind);
    let mut stream = response.bytes_stream();
    let mut complete = String::new();
    while let Some(chunk) = stream.next().await {
        let bytes = chunk.map_err(|e| format!("读取流失败：{e}"))?;
        for delta in parser.push_bytes(&bytes) {
            complete.push_str(&delta);
            on_delta(&delta);
        }
    }
    for delta in parser.finish() {
        complete.push_str(&delta);
        on_delta(&delta);
    }
    Ok(complete)
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

    /// 扩展思考：thinking 块在前，文本块在后；必须取文本块而不是 content[0]。
    #[test]
    fn anthropic_parser_skips_thinking_blocks() {
        let reply = r#"{"content":[
            {"type":"thinking","thinking":"先分析证据，再给出判断"},
            {"type":"text","text":"{\"assessment\":\"likely-false-positive\","},
            {"type":"text","text":"\"rationale\":\"测试文件中的 TLS 校验\"}"}
        ]}"#;
        let text = parse_anthropic_response(reply).unwrap();
        assert_eq!(
            text,
            r#"{"assessment":"likely-false-positive","rationale":"测试文件中的 TLS 校验"}"#
        );

        // 只有 thinking、没有文本块 → 可读错误，而不是空字符串。
        let only_thinking = r#"{"content":[{"type":"thinking","thinking":"..."}]}"#;
        let err = parse_anthropic_response(only_thinking).unwrap_err();
        assert!(err.contains("没有 text 内容块"), "{err}");
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

    #[test]
    fn sse_parsing_handles_both_providers_and_split_chunks() {
        // OpenAI：一行一个 JSON；跨 chunk 的行缓冲。
        let mut parser = SseParser::new(ProviderKind::OpenAi);
        assert!(parser.push(r#"data: {"choi"#).is_empty());
        assert_eq!(
            parser.push("ces\":[{\"delta\":{\"content\":\"He\"}}]}\n"),
            vec!["He".to_string()]
        );
        // 空 delta、[DONE]、非 JSON、注释与 event 行均被跳过。
        let chunk = "data: {\"choices\":[{\"delta\":{}}]}\n\
                     data: [DONE]\n\
                     data: not-json\n\
                     : comment\n\
                     event: message\n";
        assert!(parser.push(chunk).is_empty());

        // Anthropic：只取 content_block_delta.delta.text。
        let mut parser = SseParser::new(ProviderKind::Anthropic);
        assert_eq!(
            parser.push("data: {\"type\":\"content_block_delta\",\"delta\":{\"type\":\"text_delta\",\"text\":\"lo\"}}\n"),
            vec!["lo".to_string()]
        );
        assert!(parser
            .push("data: {\"type\":\"message_start\",\"message\":{}}\n")
            .is_empty());
        // thinking_delta 只有 thinking 字段：不得作为正文增量输出。
        assert!(parser
            .push("data: {\"type\":\"content_block_delta\",\"delta\":{\"type\":\"thinking_delta\",\"thinking\":\"推理\"}}\n")
            .is_empty());
        // 无换行结尾的残留行在 finish 时输出。
        assert!(parser.push("data: {\"type\":\"content_block_delta\",\"delta\":{\"text\":\"尾\"}}").is_empty());
        assert_eq!(parser.finish(), vec!["尾".to_string()]);
        assert!(parser.finish().is_empty());

        // 多字节字符被 chunk 边界切断时不损坏。
        let mut parser = SseParser::new(ProviderKind::OpenAi);
        let line = "data: {\"choices\":[{\"delta\":{\"content\":\"你\"}}]}\n";
        let bytes = line.as_bytes();
        let cut = line.find('你').unwrap() + 1;
        assert!(parser.push_bytes(&bytes[..cut]).is_empty());
        assert_eq!(parser.push_bytes(&bytes[cut..]), vec!["你".to_string()]);
    }

    fn test_finding(fp: &str, severity: Severity, path: &str, line: usize) -> Finding {
        Finding {
            rule_id: "r/x".into(),
            dimension: codescan_engine::model::Dimension::Quality,
            severity,
            path: path.into(),
            span: codescan_engine::model::Span {
                start: codescan_engine::model::Position { line, column: 1 },
                end: codescan_engine::model::Position { line, column: 9 },
            },
            message: "m".into(),
            match_text: "x".into(),
            captures: Vec::new(),
            degraded: false,
            flows: Vec::new(),
            fingerprint: fp.into(),
        }
    }

    #[test]
    fn batch_selection_orders_by_severity_then_path_and_caps() {
        let findings = vec![
            test_finding("info", Severity::Info, "a.ts", 1),
            test_finding("warn-b", Severity::Warning, "b.ts", 1),
            test_finding("err", Severity::Error, "z.ts", 9),
            test_finding("warn-a", Severity::Warning, "a.ts", 5),
        ];
        assert_eq!(
            select_batch_fingerprints(&findings, &[], BATCH_TRIAGE_LIMIT),
            vec!["err", "warn-a", "warn-b", "info"]
        );
        assert_eq!(
            select_batch_fingerprints(&findings, &[], 2),
            vec!["err", "warn-a"]
        );
        assert_eq!(
            select_batch_fingerprints(
                &findings,
                &["warn-b".into(), "warn-b".into(), "info".into()],
                BATCH_TRIAGE_LIMIT
            ),
            vec!["warn-b", "info"]
        );
    }

    #[test]
    fn batch_parse_marks_missing_and_extra_and_rejects_bad_assessment() {
        let expected = vec!["fp-1".to_string(), "fp-2".to_string()];
        let text = r#"```json
[
  {"fingerprint":"fp-1","assessment":"true-positive","rationale":"r1"},
  {"fingerprint":"fp-extra","assessment":"uncertain","rationale":"x"}
]
```"#;
        let out = parse_batch_triage(text, &expected).unwrap();
        assert_eq!(out.len(), 3);
        assert_eq!(out[0].fingerprint, "fp-1");
        assert!(!out[0].missing);
        assert_eq!(out[1].fingerprint, "fp-2");
        assert!(out[1].missing);
        assert_eq!(out[1].assessment, "uncertain");
        assert_eq!(out[2].fingerprint, "fp-extra");
        assert!(out[2].missing);

        // 非法 assessment → 可读错误。
        let bad = r#"[{"fingerprint":"fp-1","assessment":"bogus","rationale":""}]"#;
        assert!(parse_batch_triage(bad, &expected).is_err());
        // 缺 fingerprint → 可读错误。
        let bad = r#"[{"assessment":"uncertain","rationale":""}]"#;
        assert!(parse_batch_triage(bad, &expected).is_err());
        // 无 JSON 数组 → 可读错误。
        assert!(parse_batch_triage("nope", &expected).is_err());
    }

    #[test]
    fn taint_candidate_validation_requires_match_and_compiles() {
        let valid = r#"
rules:
  - id: custom/py-taint
    language: python
    dimension: security
    severity: error
    message: 不可信数据可能到达 eval
    taint:
      sources:
        - |
          (call
            function: (identifier) @fn (#eq? @fn "input")) @match
      sinks:
        - |
          (call
            function: (identifier) @fn (#eq? @fn "eval")) @match
      sanitizers:
        - |
          (call
            function: (identifier) @fn (#eq? @fn "int")) @match
"#;
        assert!(validate_candidate(valid).is_empty(), "{:?}", validate_candidate(valid));
        // 合法 taint 候选可经合并保存。
        let merged = merge_custom_rules(None, valid).unwrap();
        assert!(merged.contains("custom/py-taint"), "{merged}");

        // 缺 @match：引擎能编译，但候选契约要求拒绝。
        let missing_match = valid.replace(
            "(call\n            function: (identifier) @fn (#eq? @fn \"eval\")) @match",
            "(call\n            function: (identifier) @fn (#eq? @fn \"eval\"))",
        );
        let errors = validate_candidate(&missing_match);
        assert!(!errors.is_empty());
        assert!(errors.iter().any(|e| e.contains("@match")), "{errors:?}");

        // query 编译失败。
        let broken = valid.replace(
            "(call\n            function: (identifier) @fn (#eq? @fn \"eval\")) @match",
            "(definitely_not_a_node) @match",
        );
        let errors = validate_candidate(&broken);
        assert!(!errors.is_empty(), "{errors:?}");
    }
}
