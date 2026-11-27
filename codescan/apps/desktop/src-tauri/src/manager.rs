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
    global_ignore_file: Option<std::path::PathBuf>,
}

struct ActiveScan {
    token: Arc<CancelToken>,
}

/// 活动扫描登记的 RAII 守卫：离开作用域（含 panic 展开）时清除登记，
/// 扫描线程异常退出不会让应用永久停在「已有扫描在进行」。
struct ActiveRegistration(Arc<Mutex<Option<ActiveScan>>>);

impl Drop for ActiveRegistration {
    fn drop(&mut self) {
        *crate::lock_recovering(&self.0) = None;
    }
}

impl ScanManager {
    pub fn new(store: Arc<Mutex<Store>>) -> ScanManager {
        ScanManager {
            store,
            active: Arc::new(Mutex::new(None)),
            global_ignore_file: None,
        }
    }

    /// 配置应用级全局忽略文件（~/.codescan/ignore，gitignore 语法）。
    pub fn with_global_ignore(
        mut self,
        path: Option<std::path::PathBuf>,
    ) -> ScanManager {
        self.global_ignore_file = path;
        self
    }

    /// 取消当前活动扫描；没有活动扫描时什么也不做。
    pub fn cancel_active(&self) {
        if let Some(active) = crate::lock_recovering(&self.active).as_ref() {
            active.token.cancel();
        }
    }

    /// 同步执行一次完整扫描（供命令层 spawn_blocking 与无头测试复用）。
    /// progress 回调线程安全；取消 → Err(Cancelled)，不留任何历史（A11）。
    pub fn run_scan(
        &self,
        root: &std::path::Path,
        custom_rules_yaml: Option<&str>,
        packs: &[codescan_engine::packs::PackSnapshot],
        progress: &(dyn Fn(codescan_engine::Progress) + Sync),
    ) -> Result<ScanOutcome, ManagerError> {
        let token = Arc::new(CancelToken::new());
        {
            let mut active = crate::lock_recovering(&self.active);
            if active.is_some() {
                return Err(ManagerError::Busy);
            }
            *active = Some(ActiveScan { token });
        }
        let _registration = ActiveRegistration(self.active.clone());
        self.run_scan_inner(root, custom_rules_yaml, packs, progress)
    }

    /// 按扫描相同的忽略规则列出工作区文件（LLM 审查等只读消费者使用）。
    pub fn list_workspace_files(&self, root: &std::path::Path) -> Vec<(std::path::PathBuf, String)> {
        codescan_engine::scan::list_workspace_files(
            root,
            &codescan_engine::scan::ScanOptions {
                global_ignore_file: self.global_ignore_file.clone(),
            },
        )
    }

    /// 注入外部 token 的扫描入口（A11 无头测试用：预置取消令牌）。
    pub fn run_scan_with_token(
        &self,
        root: &std::path::Path,
        custom_rules_yaml: Option<&str>,
        packs: &[codescan_engine::packs::PackSnapshot],
        cancel: &CancelToken,
        progress: &(dyn Fn(codescan_engine::Progress) + Sync),
    ) -> Result<ScanOutcome, ManagerError> {
        self.run_scan_with(root, custom_rules_yaml, packs, cancel, progress)
    }

    fn run_scan_inner(
        &self,
        root: &std::path::Path,
        custom_rules_yaml: Option<&str>,
        packs: &[codescan_engine::packs::PackSnapshot],
        progress: &(dyn Fn(codescan_engine::Progress) + Sync),
    ) -> Result<ScanOutcome, ManagerError> {
        let token = {
            let guard = crate::lock_recovering(&self.active);
            guard
                .as_ref()
                .map(|a| a.token.clone())
                .unwrap_or_else(|| Arc::new(CancelToken::new()))
        };
        self.run_scan_with(root, custom_rules_yaml, packs, &token, progress)
    }

    fn run_scan_with(
        &self,
        root: &std::path::Path,
        custom_rules_yaml: Option<&str>,
        packs: &[codescan_engine::packs::PackSnapshot],
        cancel: &CancelToken,
        progress: &(dyn Fn(codescan_engine::Progress) + Sync),
    ) -> Result<ScanOutcome, ManagerError> {
        let disabled: HashSet<String> = crate::lock_recovering(&self.store)
            .disabled_rules()
            .unwrap_or_default()
            .into_iter()
            .collect();

        // 自定义规则与内置冲突/编译失败 → 拒绝启动（Domain §3.3/§4.4）。
        let engine = Engine::with_rules(custom_rules_yaml, &disabled).map_err(|errs| {
            ManagerError::EngineBuild(errs.iter().map(|e| e.to_string()).collect())
        })?;

        let started = SystemTime::now();
        let options = codescan_engine::scan::ScanOptions {
            global_ignore_file: self.global_ignore_file.clone(),
        };
        let report = engine.scan_with_options(root, &options, cancel, progress)?;
        let duration_ms = started
            .elapsed()
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);
        let started_at_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);

        // 历史快照：启用规则集 + 自定义规则内容哈希（Domain §6.1 可复现三元组）。
        let ruleset_json = serde_json::json!({
            "enabled_rules": engine.enabled_rule_ids(),
            "custom_rules_sha256": custom_rules_yaml
                .map(codescan_engine::fingerprint::content_hash),
            "packs": packs,
        })
        .to_string();

        let scan_id = crate::lock_recovering(&self.store).insert_scan(
            &root.to_string_lossy(),
            &report,
            duration_ms,
            started_at_ms,
            &ruleset_json,
        )?;
        Ok(ScanOutcome { scan_id, report })
    }
}
