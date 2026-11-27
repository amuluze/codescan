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
    /// 解析/编译错误；规则目录里的正常规则为 null。
    pub error: Option<String>,
    /// 自定义规则可在规则页编辑；内置/规则包规则只读。
    pub editable: bool,
    /// builtin / custom / 规则包 "id@version"。
    pub origin: String,
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

fn read_effective_rules() -> Result<Option<String>, String> {
    let custom = read_custom_rules();
    codescan_engine::packs::combined_rules(&data_root(), custom.as_deref())
}

/// 内置技能覆写（设置页编辑）：Rust 独占写入数据根，审查时按名覆盖内置正文。
fn review_skill_path() -> PathBuf {
    data_root().join("skills").join("llm-review.json")
}

#[derive(Serialize)]
pub struct ReviewSkillFileInfo {
    pub name: String,
    pub title: String,
    pub content: String,
    pub customized: bool,
    pub bytes: usize,
}

#[derive(Serialize)]
pub struct ReviewSkillInfo {
    pub version: String,
    pub files: Vec<ReviewSkillFileInfo>,
    pub max_file_bytes: usize,
    pub max_total_bytes: usize,
}

fn skill_file_info(name: &str, overrides: &llm::ReviewSkillOverrides) -> Option<ReviewSkillFileInfo> {
    let file = llm::review_skill_file(name)?;
    let content = overrides.body(name).to_string();
    Some(ReviewSkillFileInfo {
        name: file.name.to_string(),
        title: file.title.to_string(),
        bytes: content.len(),
        customized: overrides.is_customized(name),
        content,
    })
}

fn review_skill_info(overrides: &llm::ReviewSkillOverrides) -> ReviewSkillInfo {
    ReviewSkillInfo {
        version: llm::REVIEW_SKILL_VERSION.to_string(),
        files: llm::REVIEW_SKILL_FILES
            .iter()
            .filter_map(|file| skill_file_info(file.name, overrides))
            .collect(),
        max_file_bytes: llm::REVIEW_SKILL_MAX_FILE_BYTES,
        max_total_bytes: llm::REVIEW_SKILL_MAX_TOTAL_BYTES,
    }
}

/// 读取覆写：损坏 JSON、非法名称、空白或超限条目逐项跳过，回落内置正文。
pub fn load_review_skill_overrides(path: &std::path::Path) -> llm::ReviewSkillOverrides {
    let mut overrides = llm::ReviewSkillOverrides::default();
    let Ok(text) = std::fs::read_to_string(path) else {
        return overrides;
    };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) else {
        return overrides;
    };
    let Some(files) = value.get("files").and_then(|files| files.as_object()) else {
        return overrides;
    };
    for (name, body) in files {
        if let Some(body) = body.as_str() {
            let _ = overrides.set(name, body);
        }
    }
    overrides
}

/// 原子写入覆写；空覆写删除文件，避免留下空壳。
pub fn write_review_skill_overrides(
    path: &std::path::Path,
    overrides: &llm::ReviewSkillOverrides,
) -> Result<(), String> {
    if overrides.is_empty() {
        return match std::fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(err) => Err(format!("删除技能覆写失败：{err}")),
        };
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("创建技能目录失败：{e}"))?;
    }
    let files: serde_json::Map<String, serde_json::Value> = overrides
        .iter()
        .map(|(name, body)| (name.to_string(), serde_json::Value::String(body.to_string())))
        .collect();
    let payload = serde_json::json!({ "version": 1, "files": files });
    let text = serde_json::to_vec_pretty(&payload).map_err(|e| e.to_string())?;
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    let temp = path.with_extension(format!("json.tmp-{}-{nonce}", std::process::id()));
    std::fs::write(&temp, text).map_err(|e| format!("写入技能覆写失败：{e}"))?;
    std::fs::rename(&temp, path).map_err(|e| {
        let _ = std::fs::remove_file(&temp);
        format!("更新技能覆写失败：{e}")
    })
}

pub fn save_review_skill_override(
    path: &std::path::Path,
    name: &str,
    content: &str,
) -> Result<ReviewSkillFileInfo, String> {
    let mut overrides = load_review_skill_overrides(path);
    overrides.set(name, content)?;
    write_review_skill_overrides(path, &overrides)?;
    skill_file_info(name, &overrides).ok_or_else(|| format!("未知技能文件：{name}"))
}

pub fn reset_review_skill_override(
    path: &std::path::Path,
    name: &str,
) -> Result<ReviewSkillFileInfo, String> {
    let mut overrides = load_review_skill_overrides(path);
    overrides.clear(name)?;
    write_review_skill_overrides(path, &overrides)?;
    skill_file_info(name, &overrides).ok_or_else(|| format!("未知技能文件：{name}"))
}

#[tauri::command]
pub fn llm_review_skill_info() -> Result<ReviewSkillInfo, String> {
    Ok(review_skill_info(&load_review_skill_overrides(
        &review_skill_path(),
    )))
}

#[tauri::command]
pub fn save_llm_review_skill(
    name: String,
    content: String,
) -> Result<ReviewSkillFileInfo, String> {
    save_review_skill_override(&review_skill_path(), &name, &content)
}

#[tauri::command]
pub fn reset_llm_review_skill(name: String) -> Result<ReviewSkillFileInfo, String> {
    reset_review_skill_override(&review_skill_path(), &name)
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
    let disabled: HashSet<String> = state
        .store
        .lock()
        .unwrap()
        .disabled_rules()
        .unwrap_or_default()
        .into_iter()
        .collect();
    let custom = match read_effective_rules() {
        Ok(custom) => custom,
        Err(_) => read_custom_rules(),
    };
    let pack_origins = codescan_engine::packs::rule_origins(&data_root()).unwrap_or_default();
    let custom_ids: HashSet<String> = custom
        .as_deref()
        .map(|yaml| {
            let (specs, errors) = codescan_engine::rules::parse_rule_set_lenient(yaml);
            specs
                .into_iter()
                .map(|spec| spec.id)
                .chain(errors.into_iter().map(|error| error.rule_id))
                .collect()
        })
        .unwrap_or_default();
    // 禁用规则不参与编译校验（Domain §3.3）；目录仍包含它们以支持重新启用。
    let mut build_errors = Vec::new();
    let engine = match Engine::with_rules(custom.as_deref(), &disabled) {
        Ok(engine) => engine,
        Err(errors) => {
            build_errors = errors;
            // 启用中的坏规则不能清空整个规则页：用“全部禁用”重建目录，
            // 有效规则照常列出，坏规则错误在下面作为 error 行展示。
            let mut all_disabled: HashSet<String> = Engine::with_builtin(&HashSet::new())
                .map_err(|errs| errs.iter().map(|e| e.to_string()).collect::<Vec<_>>().join("; "))?
                .all_rule_ids()
                .into_iter()
                .collect();
            if let Some(yaml) = custom.as_deref() {
                let (specs, errors) = codescan_engine::rules::parse_rule_set_lenient(yaml);
                all_disabled.extend(specs.into_iter().map(|spec| spec.id));
                all_disabled.extend(errors.into_iter().map(|error| error.rule_id));
            }
            all_disabled.insert("<file>".into());
            all_disabled.insert("<unknown>".into());
            match Engine::with_rules(custom.as_deref(), &all_disabled) {
                Ok(engine) => engine,
                Err(more) => {
                    // id 冲突等目录级错误：至少保留内置规则列表。
                    build_errors.extend(more);
                    Engine::with_builtin(&disabled).map_err(|errs| {
                        errs.iter()
                            .map(|e| e.to_string())
                            .collect::<Vec<_>>()
                            .join("; ")
                    })?
                }
            }
        }
    };
    let mut errors_by_rule: std::collections::BTreeMap<String, Vec<String>> =
        std::collections::BTreeMap::new();
    for error in engine.rule_errors().iter().chain(build_errors.iter()) {
        errors_by_rule
            .entry(error.rule_id.clone())
            .or_default()
            .push(error.to_string());
    }

    let mut infos = Vec::new();
    for (id, language, dimension, severity, message, source) in engine.rule_catalog() {
        let error = errors_by_rule.remove(&id).map(|errors| errors.join("；"));
        infos.push(RuleInfo {
            id: id.clone(),
            language,
            dimension,
            severity,
            message: match &error {
                Some(detail) => format!("规则错误：{detail}"),
                None => message,
            },
            enabled: error.is_none() && !disabled.contains(&id),
            source,
            error,
            editable: custom_ids.contains(&id),
            origin: if custom_ids.contains(&id) {
                "custom".into()
            } else {
                pack_origins
                    .get(&id)
                    .cloned()
                    .unwrap_or_else(|| "builtin".into())
            },
        });
    }

    for (id, errors) in errors_by_rule {
        let detail = errors.join("；");
        infos.push(RuleInfo {
            id,
            language: "*".into(),
            dimension: "quality".into(),
            severity: "info".into(),
            message: format!("规则错误：{detail}"),
            enabled: false,
            source: detail.clone(),
            error: Some(detail),
            editable: true,
            origin: "custom".into(),
        });
    }
    infos.sort_by(|a, b| a.id.cmp(&b.id));
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
pub fn custom_rules_status(state: State<'_, AppState>) -> Result<Option<String>, String> {
    match std::fs::read_to_string(custom_rules_path()) {
        Ok(content) => {
            let disabled: HashSet<String> = state
                .store
                .lock()
                .unwrap()
                .disabled_rules()
                .unwrap_or_default()
                .into_iter()
                .collect();
            // 校验范围 = 启用集；禁用规则的错误由 list_rules 的 error 行展示。
            match Engine::with_rules(Some(&content), &disabled) {
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
    let effective_rules = read_effective_rules()?;
    let pack_snapshot = codescan_engine::packs::snapshots(&data_root());
    tauri::async_runtime::spawn_blocking(move || {
        let progress_app = app.clone();
        let outcome = manager.run_scan(
            &root_path,
            effective_rules.as_deref(),
            &pack_snapshot,
            &move |p| {
                let _ = progress_app.emit("scan://progress", &p);
            },
        );
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
pub fn get_scan_events(
    state: State<'_, AppState>,
    scan_id: i64,
) -> Result<Vec<codescan_engine::ScanEvent>, String> {
    state
        .store
        .lock()
        .unwrap()
        .get_events(scan_id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_scan_ruleset(
    state: State<'_, AppState>,
    scan_id: i64,
) -> Result<serde_json::Value, String> {
    let text = state
        .store
        .lock()
        .unwrap()
        .scan_ruleset_json(scan_id)
        .map_err(|error| error.to_string())?;
    serde_json::from_str(&text).map_err(|error| error.to_string())
}

#[tauri::command]
pub fn delete_scan(state: State<'_, AppState>, scan_id: i64) -> Result<(), String> {
    state
        .store
        .lock()
        .unwrap()
        .delete_scan(scan_id)
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
    let store = state.store.lock().unwrap();
    let findings = store.get_findings(scan_id).map_err(|e| e.to_string())?;
    let engine_version = store
        .scan_engine_version(scan_id)
        .map_err(|e| e.to_string())?;
    drop(store);
    // 从持久化发现重建报告骨架（SARIF 仅消费这些字段）。
    let report = codescan_engine::model::ScanReport {
        engine_version,
        findings,
        events: Vec::new(),
        stats: Default::default(),
    };
    let value = sarif::export(&report);
    let content = serde_json::to_string_pretty(&value).map_err(|e| e.to_string())?;
    std::fs::write(dest, content).map_err(|e| e.to_string())
}

/// 展示层报告导出：Markdown 与 JSON 双文件写入用户选定目录。
/// `base` 只允许 ASCII 字母数字与 `-`、`_`，防止路径穿越；导出物不进入发现/基线/门禁写入路径。
#[tauri::command]
pub fn export_report(
    dest_dir: String,
    base: String,
    markdown: String,
    json: String,
) -> Result<(), String> {
    if dest_dir.trim().is_empty() {
        return Err("导出目录不能为空".into());
    }
    let dir = PathBuf::from(&dest_dir);
    if !dir.is_dir() {
        return Err(format!("导出目录不存在：{dest_dir}"));
    }
    let valid_base = !base.is_empty()
        && base.len() <= 64
        && base
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    if !valid_base {
        return Err("导出文件名只允许字母、数字、'-' 与 '_'".into());
    }
    std::fs::write(dir.join(format!("{base}.md")), markdown).map_err(|e| e.to_string())?;
    std::fs::write(dir.join(format!("{base}.json")), json).map_err(|e| e.to_string())
}

// ---------- v3：LLM 辅助层（Domain §3a） ----------

use crate::llm;

#[derive(Serialize)]
pub struct ProfileInfo {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub base_url: String,
    pub model: String,
    pub has_key: bool,
}

#[derive(Serialize)]
pub struct ProvidersInfo {
    pub profiles: Vec<ProfileInfo>,
    /// 活动 Profile id（分诊与审查的默认值）；无 Profile 时为 null。
    pub active: Option<String>,
}

fn profile_kind_str(kind: llm::ProviderKind) -> &'static str {
    match kind {
        llm::ProviderKind::OpenAi => "openai",
        llm::ProviderKind::Anthropic => "anthropic",
    }
}

fn parse_kind(kind: &str) -> Result<llm::ProviderKind, String> {
    match kind {
        "openai" => Ok(llm::ProviderKind::OpenAi),
        "anthropic" => Ok(llm::ProviderKind::Anthropic),
        other => Err(format!("不支持的 provider kind：{other}")),
    }
}

/// 旧版单一 Provider 配置在首次读取时迁移为 `default` Profile（含密钥迁移，Domain §3a.2）。
/// 迁移条件：Profile 列表不存在且旧配置存在；迁移失败不影响读取（返回空列表）。
fn migrate_legacy_provider(store: &Store) {
    if store
        .get_meta(llm::META_PROVIDER_PROFILES)
        .ok()
        .flatten()
        .is_some()
    {
        return;
    }
    let Some(json) = store.get_meta(llm::META_PROVIDER).ok().flatten() else {
        return;
    };
    let Ok(config) = llm::ProviderConfig::from_json(&json) else {
        return;
    };
    if config.validate().is_err() {
        // 旧配置本身不完整：不迁移，由读取方按「无 Profile」报可读错误。
        return;
    }
    let profile = llm::ProviderProfile {
        id: llm::LEGACY_PROFILE_ID.into(),
        name: "默认".into(),
        kind: config.kind,
        base_url: config.base_url,
        model: config.model,
    };
    if store
        .set_meta(
            llm::META_PROVIDER_PROFILES,
            &llm::profiles_to_json(std::slice::from_ref(&profile)),
        )
        .is_err()
    {
        return;
    }
    let _ = store.set_meta(
        llm::META_ACTIVE_PROVIDER,
        &serde_json::to_string(llm::LEGACY_PROFILE_ID).unwrap_or_else(|_| "\"default\"".into()),
    );
    if let Ok(Some(key)) = store.get_secret(llm::SECRET_API_KEY) {
        let _ = store.set_secret(&llm::ProviderProfile::secret_id(llm::LEGACY_PROFILE_ID), &key);
        let _ = store.delete_secret(llm::SECRET_API_KEY);
    }
}

fn read_profiles(store: &Store) -> Vec<llm::ProviderProfile> {
    migrate_legacy_provider(store);
    store
        .get_meta(llm::META_PROVIDER_PROFILES)
        .ok()
        .flatten()
        .and_then(|json| llm::profiles_from_json(&json).ok())
        .unwrap_or_default()
}

fn read_active_id(store: &Store) -> Option<String> {
    store
        .get_meta(llm::META_ACTIVE_PROVIDER)
        .ok()
        .flatten()
        .and_then(|json| serde_json::from_str::<String>(&json).ok())
}

/// 解析调用使用的 Profile：显式 id 优先，否则活动 Profile（缺失时回落首个）。
fn resolve_profile<'a>(
    store: &Store,
    profiles: &'a [llm::ProviderProfile],
    profile_id: Option<&str>,
) -> Result<&'a llm::ProviderProfile, String> {
    if let Some(id) = profile_id {
        return profiles
            .iter()
            .find(|p| p.id == id)
            .ok_or_else(|| format!("Provider Profile 不存在：{id}"));
    }
    match read_active_id(store) {
        Some(id) => profiles
            .iter()
            .find(|p| p.id == id)
            .ok_or_else(|| format!("活动 Provider Profile 不存在：{id}")),
        None => profiles
            .first()
            .ok_or_else(|| "未配置任何 Provider Profile".to_string()),
    }
}

/// 读回指定（或活动）Profile 与其 Key（无头测试覆盖错误路径）。
pub fn prepare_provider_probe(
    store: &Store,
    profile_id: Option<&str>,
) -> Result<(llm::ProviderConfig, String), String> {
    let profiles = read_profiles(store);
    let profile = resolve_profile(store, &profiles, profile_id)?;
    let config = profile.config();
    config.validate()?;
    let key = store
        .get_secret(&llm::ProviderProfile::secret_id(&profile.id))
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("未配置 API Key（Profile：{}）", profile.name))?;
    Ok((config, key))
}

#[tauri::command]
pub fn list_providers(state: State<'_, AppState>) -> Result<ProvidersInfo, String> {
    let store = state.store.lock().unwrap();
    let profiles = read_profiles(&store);
    let mut infos = Vec::with_capacity(profiles.len());
    for profile in &profiles {
        let has_key = store
            .has_secret(&llm::ProviderProfile::secret_id(&profile.id))
            .unwrap_or(false);
        infos.push(ProfileInfo {
            id: profile.id.clone(),
            name: profile.name.clone(),
            kind: profile_kind_str(profile.kind).into(),
            base_url: profile.base_url.clone(),
            model: profile.model.clone(),
            has_key,
        });
    }
    let active = read_active_id(&store).filter(|id| profiles.iter().any(|p| &p.id == id));
    Ok(ProvidersInfo {
        profiles: infos,
        active,
    })
}

/// 新建（id 为空）或更新一个 Provider Profile，并在同一调用中处理密钥。
/// `clear_key` 优先；`api_key` 去空白后为空表示保持原密钥不变。
/// 参数平铺服务于命令层调用与测试；字段本身来自 Profile 输入契约。
#[allow(clippy::too_many_arguments)]
pub fn save_provider_profile(
    store: &Store,
    id: Option<&str>,
    name: &str,
    kind: &str,
    base_url: &str,
    model: &str,
    api_key: Option<&str>,
    clear_key: bool,
) -> Result<String, String> {
    let kind = parse_kind(kind)?;
    let mut profiles = read_profiles(store);
    let existing_id = id.map(str::trim).filter(|value| !value.is_empty());
    let id = match existing_id {
        Some(id) => {
            if !profiles.iter().any(|p| p.id == id) {
                return Err(format!("Provider Profile 不存在：{id}"));
            }
            id.to_string()
        }
        None => format!(
            "p{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis())
                .unwrap_or(0)
        ),
    };
    let profile = llm::ProviderProfile {
        id: id.clone(),
        name: name.trim().to_string(),
        kind,
        base_url: base_url.trim().to_string(),
        model: model.trim().to_string(),
    };
    profile.validate()?;
    let was_first = profiles.is_empty();
    match profiles.iter_mut().find(|p| p.id == id) {
        Some(slot) => *slot = profile,
        None => profiles.push(profile),
    }
    store
        .set_meta(
            llm::META_PROVIDER_PROFILES,
            &llm::profiles_to_json(&profiles),
        )
        .map_err(|e| e.to_string())?;
    if was_first || read_active_id(store).is_none() {
        store
            .set_meta(llm::META_ACTIVE_PROVIDER, &serde_json::to_string(&id).unwrap())
            .map_err(|e| e.to_string())?;
    }
    let secret_id = llm::ProviderProfile::secret_id(&id);
    if clear_key {
        store
            .delete_secret(&secret_id)
            .map_err(|e| format!("清除 API Key 失败：{e}"))?;
    } else if let Some(key) = api_key.map(str::trim).filter(|key| !key.is_empty()) {
        store
            .set_secret(&secret_id, key)
            .map_err(|e| format!("保存 API Key 失败：{e}"))?;
    }
    Ok(id)
}

/// 新建（id 为空）或更新一个 Provider Profile；首个 Profile 自动成为活动 Profile。
/// 参数平铺是 Tauri invoke 契约的一部分，`state` 也计入参数个数。
#[allow(clippy::too_many_arguments)]
#[tauri::command]
pub fn upsert_provider_profile(
    state: State<'_, AppState>,
    id: Option<String>,
    name: String,
    kind: String,
    base_url: String,
    model: String,
    api_key: Option<String>,
    clear_key: Option<bool>,
) -> Result<String, String> {
    let store = state.store.lock().unwrap();
    save_provider_profile(
        &store,
        id.as_deref(),
        &name,
        &kind,
        &base_url,
        &model,
        api_key.as_deref(),
        clear_key.unwrap_or(false),
    )
}

/// 删除 Profile 及其密钥；被删的是活动 Profile 时，活动改为剩余首个（无剩余则清空）。
#[tauri::command]
pub fn delete_provider_profile(state: State<'_, AppState>, id: String) -> Result<(), String> {
    let store = state.store.lock().unwrap();
    let mut profiles = read_profiles(&store);
    let before = profiles.len();
    profiles.retain(|p| p.id != id);
    if profiles.len() == before {
        return Err(format!("Provider Profile 不存在：{id}"));
    }
    store
        .set_meta(
            llm::META_PROVIDER_PROFILES,
            &llm::profiles_to_json(&profiles),
        )
        .map_err(|e| e.to_string())?;
    let _ = store.delete_secret(&llm::ProviderProfile::secret_id(&id));
    if read_active_id(&store).as_deref() == Some(id.as_str()) {
        let next = profiles
            .first()
            .map(|p| serde_json::to_string(&p.id).unwrap())
            .unwrap_or_else(|| "null".into());
        store
            .set_meta(llm::META_ACTIVE_PROVIDER, &next)
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub fn set_active_provider(state: State<'_, AppState>, id: String) -> Result<(), String> {
    let store = state.store.lock().unwrap();
    let profiles = read_profiles(&store);
    if !profiles.iter().any(|p| p.id == id) {
        return Err(format!("Provider Profile 不存在：{id}"));
    }
    store
        .set_meta(llm::META_ACTIVE_PROVIDER, &serde_json::to_string(&id).unwrap())
        .map_err(|e| e.to_string())
}

#[derive(Debug, Serialize)]
pub struct ProviderTestResult {
    pub kind: String,
    pub model: String,
    pub latency_ms: u64,
}

/// 用最小请求验证指定（或活动）Profile 的 Base URL、模型名与 API Key；不写入任何状态。
#[tauri::command]
pub async fn test_provider_connection(
    state: State<'_, AppState>,
    profile_id: Option<String>,
) -> Result<ProviderTestResult, String> {
    let (config, key) = {
        // 不跨 await 持有数据库锁。
        let store = state.store.lock().unwrap();
        prepare_provider_probe(&store, profile_id.as_deref())?
    };
    let started = std::time::Instant::now();
    llm::probe(&config, &key).await?;
    Ok(ProviderTestResult {
        kind: profile_kind_str(config.kind).into(),
        model: config.model,
        latency_ms: started.elapsed().as_millis() as u64,
    })
}

/// 审查文件选择结果：候选数与游标用于 UI 展示覆盖率与续审。
pub struct ReviewSelection {
    pub files: Vec<llm::ReviewFile>,
    pub total_bytes: usize,
    /// 通过元数据/语言粗筛的候选文件数（读取后再排除二进制）。
    pub candidates: usize,
    /// 下一个未消费的候选序号；等于 candidates 表示已到末尾。
    pub next_offset: usize,
    /// 因超大（>64 KiB）或压缩形态未纳入审查的源码文件数；工作区级常量，每页重复计算。
    pub skipped_large: usize,
}

/// 按语言轮转生成确定性候选顺序（组内保持调用方传入的相对路径顺序）。
/// 返回 (候选顺序, 因超大被排除的源码文件数)：先判语言再判大小，
/// 使超大计数只覆盖确实可识别为源码的文件。
fn review_candidate_order(files: &[(PathBuf, String)]) -> (Vec<(PathBuf, String)>, usize) {
    use std::collections::BTreeMap;

    let mut groups: BTreeMap<String, Vec<(PathBuf, String)>> = BTreeMap::new();
    let mut skipped_large = 0usize;
    for (abs, rel) in files {
        let Some(language) =
            codescan_engine::language::Language::detect(std::path::Path::new(rel))
        else {
            continue;
        };
        let Ok(metadata) = std::fs::metadata(abs) else {
            continue;
        };
        if metadata.len() > llm::REVIEW_FILE_MAX_BYTES {
            // 超大源码文件不参与审查，但计数暴露覆盖盲区（UI 摘要展示）。
            skipped_large += 1;
            continue;
        }
        groups
            .entry(language.as_str().to_string())
            .or_default()
            .push((abs.clone(), rel.clone()));
    }

    let mut cursors: BTreeMap<String, usize> = BTreeMap::new();
    let mut order = Vec::new();
    loop {
        let mut progressed = false;
        for (key, items) in &groups {
            let cursor = cursors.entry(key.clone()).or_insert(0);
            if let Some(candidate) = items.get(*cursor) {
                order.push(candidate.clone());
                *cursor += 1;
                progressed = true;
            }
        }
        if !progressed {
            break;
        }
    }
    (order, skipped_large)
}

/// 从工作区文件中取一页可审查源码：
/// - 跳过超限、不可读与二进制文件（不计入页内，但消费游标）；
/// - 压缩/生成形态（行数 < 4 且 > 16 KiB）读入后静默跳过（消费游标，与二进制同口径）：
///   按行二分对它们无效，预过滤避免 thinking 截断后整页中止；
/// - 按语言轮转顺序分页，`offset` 为候选序号；
/// - 遵守文件数与总字节上限；预算不足时保留该候选给下一页。
pub fn collect_review_files(
    files: &[(PathBuf, String)],
    max_files: usize,
    max_bytes: usize,
    offset: usize,
) -> ReviewSelection {
    let (order, skipped_large) = review_candidate_order(files);
    let candidates = order.len();
    let mut index = offset.min(candidates);
    let mut files_out = Vec::new();
    let mut total_bytes = 0usize;
    while index < candidates && files_out.len() < max_files {
        let (abs, rel) = &order[index];
        index += 1;

        let Ok(bytes) = std::fs::read(abs) else {
            continue;
        };
        if bytes[..bytes.len().min(8192)].contains(&0) {
            continue;
        }
        if total_bytes + bytes.len() > max_bytes {
            // 预算不足：保留该候选，下一页从它开始。
            index -= 1;
            break;
        }
        let content = String::from_utf8_lossy(&bytes).into_owned();
        let lines = content.lines().count().max(1);
        if lines < 4 && bytes.len() > llm::REVIEW_MINIFIED_MIN_BYTES {
            // 压缩/生成形态：按行二分到单行仍会超预算，跳过（消费游标）。
            continue;
        }
        total_bytes += bytes.len();
        files_out.push(llm::ReviewFile {
            path: rel.clone(),
            content,
            lines,
            start_line: 1,
        });
    }

    ReviewSelection {
        files: files_out,
        total_bytes,
        candidates,
        next_offset: index,
        skipped_large,
    }
}

/// 失败页只推进到已完成文件；成功页使用完整下一页游标。
pub fn review_next_offset(
    page_offset: usize,
    reviewed_files: usize,
    full_next_offset: usize,
    failed: bool,
) -> usize {
    if failed {
        page_offset.saturating_add(reviewed_files)
    } else {
        full_next_offset
    }
}

/// LLM 源码审查：只返回展示层建议，不写入 findings/基线/门禁。
/// 参数平铺是 Tauri invoke 契约的一部分，`app`/`state` 也计入参数个数。
#[allow(clippy::too_many_arguments)]
#[tauri::command]
pub async fn llm_review_workspace(
    app: AppHandle,
    state: State<'_, AppState>,
    root: String,
    max_files: Option<usize>,
    offset: Option<usize>,
    request_id: Option<String>,
    provider_id: Option<String>,
    verify: Option<bool>,
) -> Result<llm::ReviewResult, String> {
    let root_path = PathBuf::from(&root);
    if !root_path.is_dir() {
        return Err(format!("工作区不是目录：{root}"));
    }
    let (config, key) = {
        let store = state.store.lock().unwrap();
        // 审查可显式指定 Profile；缺省用活动 Profile（Domain §3a.2）。
        prepare_provider_probe(&store, provider_id.as_deref())?
    };
    let listed = state.manager.list_workspace_files(&root_path);
    let max_files = max_files
        .unwrap_or(llm::REVIEW_MAX_FILES)
        .clamp(1, llm::REVIEW_MAX_FILES);
    let page_offset = offset.unwrap_or(0);
    let selection = collect_review_files(
        &listed,
        max_files,
        llm::REVIEW_MAX_BYTES,
        page_offset,
    );
    if selection.files.is_empty() {
        return Err("工作区中没有可审查的源码文件".into());
    }
    let page_files = selection.files.len();
    let request_id = request_id.unwrap_or_default();
    // 缺省开启独立复核；显式 false 时保持单轮审查。
    let verify_enabled = verify.unwrap_or(true);
    let skill = load_review_skill_overrides(&review_skill_path());
    let outcome = llm::review_batched(
        &config,
        &key,
        &selection.files,
        &skill,
        verify_enabled,
        |done, total| {
            let _ = app.emit(
                "llm://review_progress",
                serde_json::json!({
                    "requestId": request_id,
                    "done": done,
                    "total": total,
                    "files": page_files,
                }),
            );
        },
    )
    .await;
    // 失败时只推进到已完成文件，避免跳过失败批次的源码。
    let next_offset = review_next_offset(
        page_offset,
        outcome.reviewed_files,
        selection.next_offset,
        outcome.error.is_some(),
    );
    Ok(llm::ReviewResult {
        files_reviewed: outcome.reviewed_files,
        candidate_files: selection.candidates,
        next_offset,
        total_bytes: outcome.reviewed_bytes,
        model: config.model,
        skipped_large: selection.skipped_large as u32,
        dropped: outcome.dropped,
        rejected: outcome.rejected,
        verification_enabled: verify_enabled,
        error: outcome.error,
        suggestions: outcome.suggestions,
    })
}

/// 读取一次 LLM 调用所需的全部上下文；Key 只在 Rust 内存中短暂存在。/// 读取一次 LLM 调用所需的全部上下文；Key 只在 Rust 内存中短暂存在。
/// 与 Tauri State 解耦（无头测试覆盖 C2/C7 的错误路径）。
pub fn prepare_llm_call(
    store: &Store,
    custom_rules_yaml: Option<&str>,
    scan_id: i64,
    fingerprint: &str,
) -> Result<(llm::ProviderConfig, String, llm::FindingContext), String> {
    let profiles = read_profiles(store);
    let profile = resolve_profile(store, &profiles, None)?;
    let config = profile.config();
    config.validate()?;
    let key = store
        .get_secret(&llm::ProviderProfile::secret_id(&profile.id))
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("未配置 API Key（Profile：{}）", profile.name))?;
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
    let profiles = read_profiles(store);
    let profile = resolve_profile(store, &profiles, None)?;
    let config = profile.config();
    config.validate()?;
    let key = store
        .get_secret(&llm::ProviderProfile::secret_id(&profile.id))
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("未配置 API Key（Profile：{}）", profile.name))?;
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

#[derive(Debug, Serialize)]
pub struct RuleTestHit {
    pub rule_id: String,
    pub line: usize,
    pub column: usize,
    pub message: String,
    pub match_text: String,
}

#[tauri::command]
pub fn list_rule_packs() -> Vec<codescan_engine::packs::RulePackInfo> {
    codescan_engine::packs::list(&data_root())
}

#[tauri::command]
pub fn install_rule_pack(source_dir: String) -> Result<codescan_engine::packs::RulePackInfo, String> {
    codescan_engine::packs::install(&data_root(), std::path::Path::new(&source_dir))
}

#[tauri::command]
pub fn remove_rule_pack(id: String, version: String) -> Result<(), String> {
    codescan_engine::packs::remove(&data_root(), &id, &version)
}

#[tauri::command]
pub fn get_custom_rules() -> Result<Option<String>, String> {
    match std::fs::read_to_string(custom_rules_path()) {
        Ok(content) => Ok(Some(content)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.to_string()),
    }
}

fn validate_custom_yaml(
    data_root: &std::path::Path,
    yaml: &str,
    disabled: &HashSet<String>,
) -> Vec<String> {
    if yaml.len() > 1024 * 1024 {
        return vec!["自定义规则文件超过 1 MiB 上限".into()];
    }
    let combined = match codescan_engine::packs::combined_rules(data_root, Some(yaml)) {
        Ok(combined) => combined,
        Err(error) => return vec![error],
    };
    match Engine::with_rules(combined.as_deref(), disabled) {
        Ok(_) => Vec::new(),
        Err(errors) => errors.iter().map(|error| error.to_string()).collect(),
    }
}

#[tauri::command]
pub fn validate_custom_rules(
    state: State<'_, AppState>,
    yaml: String,
) -> Result<Vec<String>, String> {
    let disabled: HashSet<String> = state
        .store
        .lock()
        .unwrap()
        .disabled_rules()
        .unwrap_or_default()
        .into_iter()
        .collect();
    Ok(validate_custom_yaml(&data_root(), &yaml, &disabled))
}

#[tauri::command]
pub fn save_custom_rules(state: State<'_, AppState>, yaml: String) -> Result<(), String> {
    let disabled: HashSet<String> = state
        .store
        .lock()
        .unwrap()
        .disabled_rules()
        .unwrap_or_default()
        .into_iter()
        .collect();
    let errors = validate_custom_yaml(&data_root(), &yaml, &disabled);
    if !errors.is_empty() {
        return Err(errors.join("\n"));
    }
    let path = custom_rules_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let temp = path.with_extension("yaml.tmp");
    std::fs::write(&temp, yaml).map_err(|e| e.to_string())?;
    if let Err(error) = std::fs::rename(&temp, &path) {
        let _ = std::fs::remove_file(&temp);
        return Err(error.to_string());
    }
    Ok(())
}

#[tauri::command]
pub fn test_custom_rule(
    candidate_yaml: String,
    sample: String,
    language: String,
) -> Result<Vec<RuleTestHit>, String> {
    if candidate_yaml.len() > 1024 * 1024 {
        return Err("测试规则超过 1 MiB 上限".into());
    }
    if sample.len() > 64 * 1024 {
        return Err("测试样例超过 64 KiB 上限".into());
    }
    let extension = match language.as_str() {
        "typescript" => "ts",
        "tsx" => "tsx",
        "javascript" => "js",
        "jsx" => "jsx",
        "python" => "py",
        "java" => "java",
        "go" => "go",
        "rust" => "rs",
        _ => return Err(format!("不支持的语言：{language}")),
    };
    let builtin_ids: HashSet<String> = Engine::with_builtin(&HashSet::new())
        .map_err(|errs| errs.iter().map(|e| e.to_string()).collect::<Vec<_>>().join("; "))?
        .all_rule_ids()
        .into_iter()
        .collect();
    let engine = Engine::with_rules(Some(&candidate_yaml), &builtin_ids)
        .map_err(|errs| errs.iter().map(|e| e.to_string()).collect::<Vec<_>>().join("\n"))?;
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|value| value.as_nanos())
        .unwrap_or_default();
    let workspace = std::env::temp_dir().join(format!(
        "codescan-rule-test-{}-{unique}",
        std::process::id()
    ));
    std::fs::create_dir_all(&workspace).map_err(|e| e.to_string())?;
    let sample_path = workspace.join(format!("sample.{extension}"));
    let result = (|| -> Result<Vec<RuleTestHit>, String> {
        std::fs::write(&sample_path, sample).map_err(|e| e.to_string())?;
        let report = engine
            .scan(&workspace, &codescan_engine::CancelToken::new(), &|_| {})
            .map_err(|e| e.to_string())?;
        Ok(report
            .findings
            .into_iter()
            .map(|finding| RuleTestHit {
                rule_id: finding.rule_id,
                line: finding.span.start.line,
                column: finding.span.start.column,
                message: finding.message,
                match_text: finding.match_text,
            })
            .collect())
    })();
    let _ = std::fs::remove_dir_all(&workspace);
    result
}
