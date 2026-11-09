//! 扫描管理器：单活动扫描、取消即作废（不落历史，A11）、成功才持久化。
//! 与 Tauri 解耦，可无头测试。

use crate::store::{Store, StoreError};
use codescan_engine::{CancelToken, Engine, ScanError, ScanReport};
use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

pub struct ScanOutcome {
    pub scan_id: i64,
    pub report: ScanReport,
}

#[derive(Debug)]
pub enum ManagerError {
    Scan(ScanError),
    Store(StoreError),
    EngineBuild(Vec<String>),
    Busy,
}

impl std::fmt::Display for ManagerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ManagerError::Scan(e) => write!(f, "{e}"),
            ManagerError::Store(e) => write!(f, "{e}"),
            ManagerError::EngineBuild(errs) => {
                write!(f, "规则编译失败：{}", errs.join("; "))
            }
            ManagerError::Busy => write!(f, "已有扫描在进行"),
        }
    }
}
impl std::error::Error for ManagerError {}
impl From<ScanError> for ManagerError {
    fn from(e: ScanError) -> Self {
        ManagerError::Scan(e)
    }
}
impl From<StoreError> for ManagerError {
    fn from(e: StoreError) -> Self {
        ManagerError::Store(e)
    }
}
#[derive(Clone)]
pub struct ScanManager {
    store: Arc<Mutex<Store>>,
    active: Arc<Mutex<Option<ActiveScan>>>,
}

struct ActiveScan {
    token: Arc<CancelToken>,
}

impl ScanManager {
    pub fn new(store: Arc<Mutex<Store>>) -> ScanManager {
        ScanManager {
            store,
            active: Arc::new(Mutex::new(None)),
        }
    }

    pub fn cancel_active(&self) {
        if let Some(active) = self.active.lock().unwrap().as_ref() {
            active.token.cancel();
        }
    }

    pub fn is_busy(&self) -> bool {
        self.active.lock().unwrap().is_some()
    }

    /// 同步执行一次完整扫描（供命令层 spawn_blocking 与无头测试复用）。
    /// progress 回调线程安全；取消 → Err(Cancelled)，不留任何历史（A11）。
    pub fn run_scan(
        &self,
        root: &std::path::Path,
        custom_rules_yaml: Option<&str>,
        progress: &(dyn Fn(codescan_engine::Progress) + Sync),
    ) -> Result<ScanOutcome, ManagerError> {
        let token = Arc::new(CancelToken::new());
        {
            let mut active = self.active.lock().unwrap();
            if active.is_some() {
                return Err(ManagerError::Busy);
            }
            *active = Some(ActiveScan { token });
        }
        let result = self.run_scan_inner(root, custom_rules_yaml, progress);
        *self.active.lock().unwrap() = None;
        result
    }

    /// 注入外部 token 的扫描入口（A11 无头测试用：预置取消令牌）。
    pub fn run_scan_with_token(
        &self,
        root: &std::path::Path,
        custom_rules_yaml: Option<&str>,
        cancel: &CancelToken,
        progress: &(dyn Fn(codescan_engine::Progress) + Sync),
    ) -> Result<ScanOutcome, ManagerError> {
        self.run_scan_with(root, custom_rules_yaml, cancel, progress)
    }

    fn run_scan_inner(
        &self,
        root: &std::path::Path,
        custom_rules_yaml: Option<&str>,
        progress: &(dyn Fn(codescan_engine::Progress) + Sync),
    ) -> Result<ScanOutcome, ManagerError> {
        let token = {
            let guard = self.active.lock().unwrap();
            guard
                .as_ref()
                .map(|a| a.token.clone())
                .unwrap_or_else(|| Arc::new(CancelToken::new()))
        };
        self.run_scan_with(root, custom_rules_yaml, &token, progress)
    }

    fn run_scan_with(
        &self,
        root: &std::path::Path,
        custom_rules_yaml: Option<&str>,
        cancel: &CancelToken,
        progress: &(dyn Fn(codescan_engine::Progress) + Sync),
    ) -> Result<ScanOutcome, ManagerError> {
        let disabled: HashSet<String> = self
            .store
            .lock()
            .unwrap()
            .disabled_rules()
            .unwrap_or_default()
            .into_iter()
            .collect();

        // 自定义规则与内置冲突/编译失败 → 拒绝启动（Domain §3.3/§4.4）。
        let engine = Engine::with_rules(custom_rules_yaml, &disabled).map_err(|errs| {
            ManagerError::EngineBuild(errs.iter().map(|e| e.to_string()).collect())
        })?;

        let started = SystemTime::now();
        let report = engine.scan(root, cancel, progress)?;
        let duration_ms = started
            .elapsed()
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);
        let started_at_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);

        let scan_id = self.store.lock().unwrap().insert_scan(
            &root.to_string_lossy(),
            &report,
            duration_ms,
            started_at_ms,
        )?;
        Ok(ScanOutcome { scan_id, report })
    }
}
