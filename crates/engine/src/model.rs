//! 发现与扫描输出的核心模型。JSON 序列化是 A1 确定性验收的比对物：
//! 任何机器差异（时间、绝对路径、并行顺序）不得进入这些结构。

use serde::{Deserialize, Serialize};

/// 三维分类（Domain §1）：规则声明的固有属性。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Dimension {
    Security,
    BusinessLogic,
    Quality,
}

/// 严重级（Domain §1），与维度正交。SARIF 映射见 sarif.rs。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Error,
    Warning,
    Info,
}

impl Severity {
    pub fn to_sarif_level(self) -> &'static str {
        match self {
            Severity::Error => "error",
            Severity::Warning => "warning",
            Severity::Info => "note",
        }
    }
}

/// 1-based 行列位置。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Position {
    pub line: usize,
    pub column: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Span {
    pub start: Position,
    pub end: Position,
}

/// 单条发现（Domain §2）。指纹由 fingerprint.rs 在排序后统一赋值。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Finding {
    pub rule_id: String,
    pub dimension: Dimension,
    pub severity: Severity,
    /// 工作区相对路径，`/` 分隔。
    pub path: String,
    pub span: Span,
    pub message: String,
    /// 命中文本（或度量实体文本）。
    pub match_text: String,
    /// 元变量捕获（capture 名 → 文本），按名排序后序列化保证字节稳定。
    pub captures: Vec<(String, String)>,
    /// 容错解析下的命中（Domain §3.2）。
    #[serde(default)]
    pub degraded: bool,
    /// Domain §2.3 定义的指纹；同一报告内唯一。
    pub fingerprint: String,
}

/// 扫描事件（Domain §3.1）：逐文件事件与汇总事件，显式记录一切未产出发现的输入。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ScanEvent {
    /// 逐文件：解析失败（容错解析继续，命中标记 degraded）。
    ParseError { path: String, error_nodes: usize },
    /// 逐文件：读取失败。
    IoError { path: String, message: String },
    /// 逐文件：二进制文件（前 8 KiB 含 NUL）。
    Binary { path: String },
    /// 逐文件：超过 1 MiB 引擎常量上限。
    Oversize { path: String, size: u64 },
    /// 汇总：被忽略规则/隐藏属性排除的文件数（不含 `.git`）。
    IgnoredSummary { count: u64 },
    /// 汇总：未知语言扩展名的文件数。
    UnknownLanguageSummary { count: u64 },
}

impl ScanEvent {
    /// 稳定排序键：类别序 + 路径。不依赖遍历顺序。
    fn sort_key(&self) -> (u8, String) {
        match self {
            ScanEvent::ParseError { path, .. }
            | ScanEvent::IoError { path, .. }
            | ScanEvent::Binary { path }
            | ScanEvent::Oversize { path, .. } => (0, path.clone()),
            ScanEvent::IgnoredSummary { .. } => (1, String::new()),
            ScanEvent::UnknownLanguageSummary { .. } => (2, String::new()),
        }
    }

    pub fn sort(events: &mut [ScanEvent]) {
        events.sort_by_key(ScanEvent::sort_key);
    }
}

#[derive(Debug)]
pub enum ScanError {
    Cancelled,
    Io(std::io::Error),
}

impl std::fmt::Display for ScanError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ScanError::Cancelled => write!(f, "扫描已取消"),
            ScanError::Io(e) => write!(f, "IO 错误：{e}"),
        }
    }
}

impl std::error::Error for ScanError {}

/// 扫描统计（仅确定性计数；耗时等机器相关量不入报告，由 UI 自行测量）。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScanStats {
    pub files_total: u64,
    pub files_scanned: u64,
    pub files_ignored: u64,
    pub files_unknown_language: u64,
    pub findings: u64,
}

/// 扫描报告：确定性产物的完整载体。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScanReport {
    pub engine_version: String,
    pub findings: Vec<Finding>,
    pub events: Vec<ScanEvent>,
    pub stats: ScanStats,
}

/// 扫描进度回调载荷。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "stage", rename_all = "snake_case")]
pub enum Progress {
    Walking,
    Scanning { done: u64, total: u64 },
    Finalizing,
}

/// 取消令牌：置位后扫描在下一个文件边界终止。
#[derive(Debug, Default)]
pub struct CancelToken {
    cancelled: std::sync::atomic::AtomicBool,
}

impl CancelToken {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, std::sync::atomic::Ordering::SeqCst);
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(std::sync::atomic::Ordering::SeqCst)
    }
}

