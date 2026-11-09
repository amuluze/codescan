//! codescan-engine：确定性本地 AST 规则引擎。
//!
//! 领域不变量见 `.specs/domain/scan-engine.md`：可复现、可定位、指纹稳定、
//! fail-closed、扫描只读。本 crate 无 Tauri/IO 副作用依赖（工作区文件读取除外），
//! 全部能力可无头测试。

pub mod baseline;
pub mod compile;
pub mod fingerprint;
pub mod language;
pub mod metrics;
pub mod model;
pub mod rules;
pub mod sarif;
pub mod scan;
pub mod taint;

pub use model::{
    CancelToken, Dimension, Finding, Position, Progress, ScanError, ScanEvent, ScanReport,
    ScanStats, Severity, Span,
};

pub use scan::{Engine, MAX_FILE_BYTES};