//! Tauri 命令层：薄胶水。业务在 Store/ScanManager/Engine，均无头可测。
//! 事件：`scan://progress`（阶段进度）、`scan://finished`（完成，载荷含 scan_id）。

use crate::manager::{ManagerError, ScanManager};
use crate::store::{ScanSummary, Store, StoreError};
use codescan_engine::model::Finding;
use codescan_engine::{sarif, Engine};
use serde::Serialize;
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, State};

pub struct AppState {
    pub store: Arc<Mutex<Store>>,
    pub manager: ScanManager,
}

#[derive(Serialize)]
pub struct RuleInfo {
    pub id: String,
    pub language: String,
    pub dimension: String,
    pub severity: String,
    pub message: String,
    pub enabled: bool,
    pub source: String,
}

#[derive(Serialize)]
pub struct ScanFinished {
    pub scan_id: i64,
    pub findings: Vec<Finding>,
    pub stats: codescan_engine::model::ScanStats,
}

fn data_root() -> PathBuf {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    home.join(".codescan")
}

fn custom_rules_path() -> PathBuf {
    data_root().join("rules").join("custom.yaml")
}

fn read_custom_rules() -> Option<String> {
    std::fs::read_to_string(custom_rules_path()).ok()
}

/// 目录浏览（应用内目录选择器）：返回规范化路径、父目录与直属子目录。
/// 不依赖原生对话框，行为在所有运行环境一致。
#[derive(Serialize)]
pub struct DirEntry {
    pub name: String,
    pub path: String,
}

#[derive(Serialize)]
pub struct DirListing {
    pub path: String,
    pub parent: Option<String>,
    pub entries: Vec<DirEntry>,
}

#[tauri::command]
pub fn list_directory(path: Option<String>) -> Result<DirListing, String> {
    let home = std::env::var_os("HOME")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from("/"));
    let requested = path
        .filter(|p| !p.trim().is_empty())
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| home.clone());
    let canonical = requested
        .canonicalize()
        .map_err(|err| format!("无法访问目录 {}：{err}", requested.display()))?;
    if !canonical.is_dir() {
        return Err(format!("不是目录：{}", canonical.display()));
    }
    let mut entries = Vec::new();
    let read = std::fs::read_dir(&canonical)
        .map_err(|err| format!("读取目录失败 {}：{err}", canonical.display()))?;
    for entry in read.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue; // 隐藏目录不列出；可手动输入路径访问
        }
        // path().is_dir() 跟随符号链接：macOS 的 /tmp、/var 等是链接，不能漏列。
        if !entry.path().is_dir() {
            continue;
        }
        entries.push(DirEntry {
            name,
            path: entry.path().to_string_lossy().into_owned(),
        });
    }
    entries.sort_by_key(|a| a.name.to_lowercase());
    Ok(DirListing {
        path: canonical.to_string_lossy().into_owned(),
        parent: canonical
            .parent()
            .map(|p| p.to_string_lossy().into_owned()),
        entries,
    })
}

#[tauri::command]
pub fn list_rules(state: State<'_, AppState>) -> Result<Vec<RuleInfo>, String> {
    let engine = Engine::with_rules(read_custom_rules().as_deref(), &HashSet::new())
        .map_err(|errs| errs.iter().map(|e| e.to_string()).collect::<Vec<_>>().join("; "))?;
    let mut infos = Vec::new();
    let disabled: HashSet<String> = state
        .store
        .lock()
        .unwrap()
        .disabled_rules()
        .unwrap_or_default()
        .into_iter()
        .collect();
    for (id, language, dimension, severity, message, source) in engine.rule_catalog() {
        infos.push(RuleInfo {
            id: id.clone(),
            language,
            dimension,
            severity,
            message,
            enabled: !disabled.contains(&id),
            source,
        });
    }
    Ok(infos)
}

#[tauri::command]
pub fn set_rule_enabled(
    state: State<'_, AppState>,
    rule_id: String,
    enabled: bool,
) -> Result<(), String> {
    state
        .store
        .lock()
        .unwrap()
        .set_rule_enabled(&rule_id, enabled)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn custom_rules_status() -> Result<Option<String>, String> {
    match std::fs::read_to_string(custom_rules_path()) {
        Ok(content) => {
            // 编译校验（启停无关）；错误以文本返回供规则管理页展示。
            match Engine::with_rules(Some(&content), &HashSet::new()) {
                Ok(_) => Ok(Some(content)),
                Err(errs) => Err(errs.iter().map(|e| e.to_string()).collect::<Vec<_>>().join("\n")),
            }
        }
        Err(_) => Ok(None),
    }
}

#[tauri::command]
pub async fn start_scan(
    app: AppHandle,
    state: State<'_, AppState>,
    root: String,
) -> Result<(), String> {
    // State 借用不能跨 await：克隆内部 Arc 后再 spawn。
    let manager = state.manager.clone();
    let app = app.clone();
    let root_path = PathBuf::from(root);
    tauri::async_runtime::spawn_blocking(move || {
        let progress_app = app.clone();
        let outcome = manager.run_scan(&root_path, read_custom_rules().as_deref(), &move |p| {
            let _ = progress_app.emit("scan://progress", &p);
        });
        match outcome {
            Ok(result) => {
                let _ = app.emit(
                    "scan://finished",
                    &ScanFinished {
                        scan_id: result.scan_id,
                        stats: result.report.stats,
                        findings: Vec::new(), // 完整发现经 get_scan 拉取
                    },
                );
            }
            Err(ManagerError::Scan(codescan_engine::ScanError::Cancelled)) => {
                let _ = app.emit("scan://cancelled", ());
            }
            Err(e) => {
                let _ = app.emit("scan://error", e.to_string());
            }
        }
    });
    Ok(())
}

#[tauri::command]
pub fn cancel_scan(state: State<'_, AppState>) {
    state.manager.cancel_active();
}

#[tauri::command]
pub fn list_scans(state: State<'_, AppState>) -> Result<Vec<ScanSummary>, String> {
    state.store.lock().unwrap().list_scans().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_scan(
    state: State<'_, AppState>,
    scan_id: i64,
) -> Result<Vec<Finding>, String> {
    state
        .store
        .lock()
        .unwrap()
        .get_findings(scan_id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn set_baseline(state: State<'_, AppState>, scan_id: i64) -> Result<i64, String> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);
    state
        .store
        .lock()
        .unwrap()
        .set_baseline(scan_id, now)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn gate_diff(
    state: State<'_, AppState>,
    scan_id: i64,
) -> Result<Option<Vec<Finding>>, String> {
    state.store.lock().unwrap().gate_new(scan_id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn baseline_info(state: State<'_, AppState>) -> Result<Option<serde_json::Value>, String> {
    let store = state.store.lock().unwrap();
    match store.current_baseline() {
        Ok(Some(b)) => Ok(Some(serde_json::json!({
            "engine_version": b.engine_version,
            "entries": b.entries.len(),
        }))),
        Ok(None) => Ok(None),
        Err(StoreError::Db(e)) => Err(e.to_string()),
        Err(e) => Err(e.to_string()),
    }
}

#[tauri::command]
pub fn export_sarif(state: State<'_, AppState>, scan_id: i64, dest: String) -> Result<(), String> {
    let findings = state
        .store
        .lock()
        .unwrap()
        .get_findings(scan_id)
        .map_err(|e| e.to_string())?;
    // 从持久化发现重建报告骨架（SARIF 仅消费这些字段）。
    let report = codescan_engine::model::ScanReport {
        engine_version: "persisted".into(),
        findings,
        events: Vec::new(),
        stats: Default::default(),
    };
    let value = sarif::export(&report);
    let content = serde_json::to_string_pretty(&value).map_err(|e| e.to_string())?;
    std::fs::write(dest, content).map_err(|e| e.to_string())
}

// ---------- v3：LLM 辅助层（Domain §3a） ----------

use crate::llm;

#[derive(Serialize)]
pub struct ProviderInfo {
    pub kind: String,
    pub base_url: String,
    pub model: String,
    pub has_key: bool,
}

fn read_provider_config(store: &Store) -> llm::ProviderConfig {
    store
        .get_meta(llm::META_PROVIDER)
        .ok()
        .flatten()
        .and_then(|json| llm::ProviderConfig::from_json(&json).ok())
        .unwrap_or_default()
}

#[tauri::command]
pub fn set_provider_config(
    state: State<'_, AppState>,
    kind: String,
    base_url: String,
    model: String,
) -> Result<(), String> {
    let kind = match kind.as_str() {
        "openai" => llm::ProviderKind::OpenAi,
        "anthropic" => llm::ProviderKind::Anthropic,
        other => return Err(format!("不支持的 provider kind：{other}")),
    };
    let config = llm::ProviderConfig {
        kind,
        base_url: base_url.trim().to_string(),
        model: model.trim().to_string(),
    };
    config.validate()?;
    state
        .store
        .lock()
        .unwrap()
        .set_meta(llm::META_PROVIDER, &config.to_json())
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_provider_config(state: State<'_, AppState>) -> Result<ProviderInfo, String> {
    let store = state.store.lock().unwrap();
    let config = read_provider_config(&store);
    let has_key = store
        .has_secret(llm::SECRET_API_KEY)
        .map_err(|e| e.to_string())?;
    Ok(ProviderInfo {
        kind: match config.kind {
            llm::ProviderKind::OpenAi => "openai".into(),
            llm::ProviderKind::Anthropic => "anthropic".into(),
        },
        base_url: config.base_url,
        model: config.model,
        has_key,
    })
}

#[tauri::command]
pub fn set_api_key(state: State<'_, AppState>, key: String) -> Result<(), String> {
    let key = key.trim();
    if key.is_empty() {
        return Err("API Key 不能为空".into());
    }
    state
        .store
        .lock()
        .unwrap()
        .set_secret(llm::SECRET_API_KEY, key)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn clear_api_key(state: State<'_, AppState>) -> Result<(), String> {
    state
        .store
        .lock()
        .unwrap()
        .delete_secret(llm::SECRET_API_KEY)
        .map_err(|e| e.to_string())
}

#[derive(Debug, Serialize)]
pub struct ProviderTestResult {
    pub kind: String,
    pub model: String,
    pub latency_ms: u64,
}

/// 读懂当前已保存的 Provider 配置与 Key（无头测试覆盖错误路径）。
pub fn prepare_provider_probe(
    store: &Store,
) -> Result<(llm::ProviderConfig, String), String> {
    let config = read_provider_config(store);
    config.validate()?;
    let key = store
        .get_secret(llm::SECRET_API_KEY)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "未配置 API Key".to_string())?;
    Ok((config, key))
}

/// 用最小请求验证 Base URL、模型名与 API Key 是否可用；不写入任何状态。
#[tauri::command]
pub async fn test_provider_connection(
    state: State<'_, AppState>,
) -> Result<ProviderTestResult, String> {
    let (config, key) = {
        // 不跨 await 持有数据库锁。
        let store = state.store.lock().unwrap();
        prepare_provider_probe(&store)?
    };
    let started = std::time::Instant::now();
    llm::probe(&config, &key).await?;
    Ok(ProviderTestResult {
        kind: match config.kind {
            llm::ProviderKind::OpenAi => "openai".into(),
            llm::ProviderKind::Anthropic => "anthropic".into(),
        },
        model: config.model,
        latency_ms: started.elapsed().as_millis() as u64,
    })
}

/// 读取一次 LLM 调用所需的全部上下文；Key 只在 Rust 内存中短暂存在。
/// 与 Tauri State 解耦（无头测试覆盖 C2/C7 的错误路径）。
pub fn prepare_llm_call(
    store: &Store,
    custom_rules_yaml: Option<&str>,
    scan_id: i64,
    fingerprint: &str,
) -> Result<(llm::ProviderConfig, String, llm::FindingContext), String> {
    let config = read_provider_config(store);
    config.validate()?;
    let key = store
        .get_secret(llm::SECRET_API_KEY)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "未配置 API Key".to_string())?;
    let findings = store.get_findings(scan_id).map_err(|e| e.to_string())?;
    let finding = findings
        .iter()
        .find(|f| f.fingerprint == fingerprint)
        .ok_or_else(|| "未找到该发现".to_string())?;

    let engine = Engine::with_rules(custom_rules_yaml, &HashSet::new())
        .map_err(|errs| errs.iter().map(|e| e.to_string()).collect::<Vec<_>>().join("; "))?;
    let catalog = engine.rule_catalog();
    let entry = catalog.iter().find(|(id, ..)| id == &finding.rule_id);
    let (language, dimension, severity, rule_source) = match entry {
        Some((_, language, dimension, severity, _, source)) => (
            language.clone(),
            dimension.clone(),
            severity.clone(),
            source.clone(),
        ),
        None => (
            "*".to_string(),
            serde_json::to_string(&finding.dimension).unwrap_or_default(),
            serde_json::to_string(&finding.severity).unwrap_or_default(),
            String::new(),
        ),
    };
    let dimension = dimension.trim_matches('"').to_string();
    let severity = severity.trim_matches('"').to_string();
    Ok((
        config,
        key,
        llm::FindingContext {
            rule_id: finding.rule_id.clone(),
            language,
            dimension,
            severity,
            message: finding.message.clone(),
            path: finding.path.clone(),
            line: finding.span.start.line,
            match_text: finding.match_text.clone(),
            rule_source,
        },
    ))
}

#[tauri::command]
pub async fn llm_triage(
    state: State<'_, AppState>,
    scan_id: i64,
    fingerprint: String,
) -> Result<llm::TriageResult, String> {
    let custom = read_custom_rules();
    let (config, key, context) = {
        let store = state.store.lock().unwrap();
        prepare_llm_call(&store, custom.as_deref(), scan_id, &fingerprint)?
    };
    let (system, user) = llm::triage_prompt(&context);
    let reply = llm::complete(&config, &key, &system, &user).await?;
    llm::parse_triage(&reply)
}

/// 流式增量事件载荷（requestId 用于 UI 过滤并发请求）。
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LlmDeltaPayload {
    pub request_id: String,
    pub text: String,
}

/// 流式分诊：逐片 emit `llm://delta`，最终返回完整文本。
#[tauri::command]
pub async fn llm_triage_stream(
    app: AppHandle,
    state: State<'_, AppState>,
    scan_id: i64,
    fingerprint: String,
    request_id: String,
) -> Result<String, String> {
    let custom = read_custom_rules();
    let (config, key, context) = {
        let store = state.store.lock().unwrap();
        prepare_llm_call(&store, custom.as_deref(), scan_id, &fingerprint)?
    };
    let (system, user) = llm::triage_prompt(&context);
    let reply = llm::complete_stream(&config, &key, &system, &user, move |text| {
        let _ = app.emit(
            "llm://delta",
            LlmDeltaPayload {
                request_id: request_id.clone(),
                text: text.to_string(),
            },
        );
    })
    .await?;
    Ok(reply)
}

/// 读取批量分诊所需上下文：显式指纹去重（超过上限拒绝），
/// 传空则按 severity error>warning>info、路径序取前 20。只读，不写库。
pub fn prepare_batch_llm_call(
    store: &Store,
    scan_id: i64,
    fingerprints: &[String],
) -> Result<(llm::ProviderConfig, String, Vec<Finding>), String> {
    if fingerprints.len() > llm::BATCH_TRIAGE_LIMIT {
        return Err(format!(
            "批量分诊最多 {} 条，收到 {} 条",
            llm::BATCH_TRIAGE_LIMIT,
            fingerprints.len()
        ));
    }
    let config = read_provider_config(store);
    config.validate()?;
    let key = store
        .get_secret(llm::SECRET_API_KEY)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "未配置 API Key".to_string())?;
    let findings = store.get_findings(scan_id).map_err(|e| e.to_string())?;
    let selected_fps = llm::select_batch_fingerprints(
        &findings,
        fingerprints,
        llm::BATCH_TRIAGE_LIMIT,
    );
    let mut selected = Vec::with_capacity(selected_fps.len());
    for fingerprint in selected_fps {
        let finding = findings
            .iter()
            .find(|f| f.fingerprint == fingerprint)
            .ok_or_else(|| format!("未找到该发现：{fingerprint}"))?;
        selected.push(finding.clone());
    }
    if selected.is_empty() {
        return Err("该扫描没有可分诊的发现".to_string());
    }
    Ok((config, key, selected))
}

#[tauri::command]
pub async fn llm_batch_triage(
    state: State<'_, AppState>,
    scan_id: i64,
    fingerprints: Vec<String>,
) -> Result<Vec<llm::BatchTriageEntry>, String> {
    let (config, key, findings) = {
        let store = state.store.lock().unwrap();
        prepare_batch_llm_call(&store, scan_id, &fingerprints)?
    };
    let expected: Vec<String> = findings.iter().map(|f| f.fingerprint.clone()).collect();
    let (system, user) = llm::batch_triage_prompt(&findings);
    let reply = llm::complete(&config, &key, &system, &user).await?;
    llm::parse_batch_triage(&reply, &expected)
}

#[tauri::command]
pub async fn llm_generate_rule(
    state: State<'_, AppState>,
    scan_id: i64,
    fingerprint: String,
) -> Result<llm::CandidateRule, String> {
    let custom = read_custom_rules();
    let (config, key, context) = {
        let store = state.store.lock().unwrap();
        prepare_llm_call(&store, custom.as_deref(), scan_id, &fingerprint)?
    };
    let (system, user) = llm::candidate_prompt(&context);
    let reply = llm::complete(&config, &key, &system, &user).await?;
    match llm::extract_rule_yaml(&reply) {
        Ok(yaml) => {
            let errors = llm::validate_candidate(&yaml);
            Ok(llm::CandidateRule {
                valid: errors.is_empty(),
                errors,
                yaml,
            })
        }
        Err(e) => Ok(llm::CandidateRule {
            yaml: reply,
            valid: false,
            errors: vec![e],
        }),
    }
}

/// v3.3：候选规则隔离回归。临时语料 + `ruleid` 标注 + 仅候选规则扫描；
/// 只读扫描历史/基线，不写 findings。
#[tauri::command]
pub fn llm_candidate_regression(
    state: State<'_, AppState>,
    scan_id: i64,
    fingerprint: String,
    candidate_yaml: String,
    save_fixture: bool,
) -> Result<crate::regression::CandidateRegressionResult, String> {
    let store = state.store.lock().unwrap();
    crate::regression::run_from_store(
        &store,
        &data_root(),
        scan_id,
        &fingerprint,
        &candidate_yaml,
        save_fixture,
    )
}

#[tauri::command]
pub fn save_custom_rule(state: State<'_, AppState>, yaml: String) -> Result<(), String> {
    let _ = &state;
    let existing = std::fs::read_to_string(custom_rules_path()).ok();
    let merged = llm::merge_custom_rules(existing.as_deref(), &yaml)
        .map_err(|errs| errs.join("\n"))?;
    if let Some(parent) = custom_rules_path().parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::write(custom_rules_path(), merged).map_err(|e| e.to_string())
}
