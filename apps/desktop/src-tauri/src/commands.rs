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
