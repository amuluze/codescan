//! SQLite 持久化（Rust 独占，axiom 同构）：扫描历史、规则启停、基线。
//! 纯同步实现，脱离 Tauri 可无头测试（A8）。发现 JSON 行与引擎模型一一对应。

use codescan_engine::model::{Dimension, Finding, Position, ScanEvent, ScanReport, Severity, Span};
use codescan_engine::baseline::Baseline;
use rusqlite::{params, Connection, OptionalExtension};
use std::path::Path;

#[derive(Debug)]
pub enum StoreError {
    Db(rusqlite::Error),
    NotFound(&'static str),
}

impl std::fmt::Display for StoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StoreError::Db(e) => write!(f, "数据库错误：{e}"),
            StoreError::NotFound(what) => write!(f, "未找到{what}"),
        }
    }
}
impl std::error::Error for StoreError {}
impl From<rusqlite::Error> for StoreError {
    fn from(e: rusqlite::Error) -> Self {
        StoreError::Db(e)
    }
}
type Result<T, E = StoreError> = std::result::Result<T, E>;

pub struct Store {
    conn: Connection,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ScanSummary {
    pub id: i64,
    pub root: String,
    pub engine_version: String,
    pub started_at_ms: i64,
    pub duration_ms: i64,
    pub findings: u64,
    pub files_scanned: u64,
}

impl Store {
    pub fn open(path: &Path) -> Result<Store> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| StoreError::Db(rusqlite::Error::ToSqlConversionFailure(e.into())))?;
        }
        let conn = Connection::open(path)?;
        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA foreign_keys = ON;",
        )?;
        let store = Store { conn };
        store.migrate()?;
        Ok(store)
    }

    fn migrate(&self) -> Result<()> {
        self.conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS scans(
               id INTEGER PRIMARY KEY AUTOINCREMENT,
               root TEXT NOT NULL,
               engine_version TEXT NOT NULL,
               started_at_ms INTEGER NOT NULL,
               duration_ms INTEGER NOT NULL,
               stats_json TEXT NOT NULL
             );
             CREATE TABLE IF NOT EXISTS findings(
               scan_id INTEGER NOT NULL REFERENCES scans(id) ON DELETE CASCADE,
               rule_id TEXT NOT NULL,
               dimension TEXT NOT NULL,
               severity TEXT NOT NULL,
               path TEXT NOT NULL,
               start_line INTEGER NOT NULL,
               start_col INTEGER NOT NULL,
               end_line INTEGER NOT NULL,
               end_col INTEGER NOT NULL,
               message TEXT NOT NULL,
               match_text TEXT NOT NULL,
               captures_json TEXT NOT NULL,
               degraded INTEGER NOT NULL,
               fingerprint TEXT NOT NULL
             );
             CREATE INDEX IF NOT EXISTS idx_findings_scan ON findings(scan_id);
             CREATE TABLE IF NOT EXISTS rule_states(
               rule_id TEXT PRIMARY KEY,
               enabled INTEGER NOT NULL
             );
             CREATE TABLE IF NOT EXISTS baselines(
               id INTEGER PRIMARY KEY AUTOINCREMENT,
               created_at_ms INTEGER NOT NULL,
               scan_id INTEGER NOT NULL REFERENCES scans(id) ON DELETE CASCADE,
               engine_version TEXT NOT NULL,
               entries_json TEXT NOT NULL
             );
             CREATE TABLE IF NOT EXISTS active_baseline(
               key INTEGER PRIMARY KEY CHECK(key = 1),
               baseline_id INTEGER NOT NULL REFERENCES baselines(id) ON DELETE CASCADE
             );",
        )?;
        Ok(())
    }

    pub fn insert_scan(
        &self,
        root: &str,
        report: &ScanReport,
        duration_ms: i64,
        started_at_ms: i64,
    ) -> Result<i64> {
        let stats_json = serde_json::to_string(&report.stats).map_err(|e| {
            StoreError::Db(rusqlite::Error::ToSqlConversionFailure(e.into()))
        })?;
        self.conn.execute(
            "INSERT INTO scans(root, engine_version, started_at_ms, duration_ms, stats_json)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![root, report.engine_version, started_at_ms, duration_ms, stats_json],
        )?;
        let scan_id = self.conn.last_insert_rowid();
        let mut stmt = self.conn.prepare(
            "INSERT INTO findings(scan_id, rule_id, dimension, severity, path,
                start_line, start_col, end_line, end_col, message, match_text,
                captures_json, degraded, fingerprint)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14)",
        )?;
        for f in &report.findings {
            let captures = serde_json::to_string(&f.captures).map_err(|e| {
                StoreError::Db(rusqlite::Error::ToSqlConversionFailure(e.into()))
            })?;
            stmt.execute(params![
                scan_id,
                f.rule_id,
                serde_json::to_string(&f.dimension).unwrap(),
                serde_json::to_string(&f.severity).unwrap(),
                f.path,
                f.span.start.line as i64,
                f.span.start.column as i64,
                f.span.end.line as i64,
                f.span.end.column as i64,
                f.message,
                f.match_text,
                captures,
                f.degraded as i64,
                f.fingerprint,
            ])?;
        }
        Ok(scan_id)
    }

    pub fn list_scans(&self) -> Result<Vec<ScanSummary>> {
        let mut stmt = self.conn.prepare(
            "SELECT s.id, s.root, s.engine_version, s.started_at_ms, s.duration_ms, s.stats_json
             FROM scans s ORDER BY s.id DESC LIMIT 200",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, i64>(3)?,
                row.get::<_, i64>(4)?,
                row.get::<_, String>(5)?,
            ))
        })?;
        let mut out = Vec::new();
        for row in rows {
            let (id, root, engine_version, started_at_ms, duration_ms, stats_json) = row?;
            let stats: serde_json::Value =
                serde_json::from_str(&stats_json).unwrap_or(serde_json::Value::Null);
            out.push(ScanSummary {
                id,
                root,
                engine_version,
                started_at_ms,
                duration_ms,
                findings: stats["findings"].as_u64().unwrap_or(0),
                files_scanned: stats["files_scanned"].as_u64().unwrap_or(0),
            });
        }
        Ok(out)
    }

    pub fn get_findings(&self, scan_id: i64) -> Result<Vec<Finding>> {
        let exists: bool = self
            .conn
            .query_row("SELECT 1 FROM scans WHERE id = ?1", [scan_id], |_| Ok(true))
            .optional()?
            .unwrap_or(false);
        if !exists {
            return Err(StoreError::NotFound("该扫描记录"));
        }
        let mut stmt = self.conn.prepare(
            "SELECT rule_id, dimension, severity, path, start_line, start_col, end_line, end_col,
                    message, match_text, captures_json, degraded, fingerprint
             FROM findings WHERE scan_id = ?1 ORDER BY rowid",
        )?;
        let rows = stmt.query_map([scan_id], |row| {
            Ok(FindingRow {
                rule_id: row.get(0)?,
                dimension: row.get(1)?,
                severity: row.get(2)?,
                path: row.get(3)?,
                start_line: row.get(4)?,
                start_col: row.get(5)?,
                end_line: row.get(6)?,
                end_col: row.get(7)?,
                message: row.get(8)?,
                match_text: row.get(9)?,
                captures_json: row.get(10)?,
                degraded: row.get(11)?,
                fingerprint: row.get(12)?,
            })
        })?;
        let mut findings = Vec::new();
        for row in rows {
            let r = row?;
            let dimension: Dimension = serde_json::from_str(&r.dimension).map_err(|e| {
                StoreError::Db(rusqlite::Error::ToSqlConversionFailure(e.into()))
            })?;
            let severity: Severity = serde_json::from_str(&r.severity).map_err(|e| {
                StoreError::Db(rusqlite::Error::ToSqlConversionFailure(e.into()))
            })?;
            let captures: Vec<(String, String)> =
                serde_json::from_str(&r.captures_json).unwrap_or_default();
            findings.push(Finding {
                rule_id: r.rule_id,
                dimension,
                severity,
                path: r.path,
                span: Span {
                    start: Position {
                        line: r.start_line as usize,
                        column: r.start_col as usize,
                    },
                    end: Position {
                        line: r.end_line as usize,
                        column: r.end_col as usize,
                    },
                },
                message: r.message,
                match_text: r.match_text,
                captures,
                degraded: r.degraded != 0,
                fingerprint: r.fingerprint,
            });
        }
        Ok(findings)
    }

    pub fn get_events(&self, scan_id: i64) -> Result<Vec<ScanEvent>> {
        let stats_json: Option<String> = self
            .conn
            .query_row(
                "SELECT stats_json FROM scans WHERE id = ?1",
                [scan_id],
                |row| row.get(0),
            )
            .optional()?;
        let _ = stats_json;
        // 事件不单独落表（与发现同源的扫描事件在报告中）；v1 通过 stats 复述，
        // 完整事件列表由扫描完成时直接返回前端并随 scans.stats_json 存档。
        Ok(Vec::new())
    }

    pub fn set_rule_enabled(&self, rule_id: &str, enabled: bool) -> Result<()> {
        self.conn.execute(
            "INSERT INTO rule_states(rule_id, enabled) VALUES (?1, ?2)
             ON CONFLICT(rule_id) DO UPDATE SET enabled = excluded.enabled",
            params![rule_id, enabled as i64],
        )?;
        Ok(())
    }

    pub fn disabled_rules(&self) -> Result<Vec<String>> {
        let mut stmt = self.conn.prepare("SELECT rule_id FROM rule_states WHERE enabled = 0")?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    pub fn list_rule_states(&self) -> Result<Vec<(String, bool)>> {
        let mut stmt = self.conn.prepare("SELECT rule_id, enabled FROM rule_states")?;
        let rows = stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?;
        let mut out = Vec::new();
        for row in rows {
            let (id, enabled): (String, i64) = row?;
            out.push((id, enabled != 0));
        }
        Ok(out)
    }

    pub fn set_baseline(&self, scan_id: i64, now_ms: i64) -> Result<i64> {
        let findings = self.get_findings(scan_id)?;
        let mut entries: Vec<String> = findings.into_iter().map(|f| f.fingerprint).collect();
        entries.sort();
        entries.dedup();
        let engine_version: String = self.conn.query_row(
            "SELECT engine_version FROM scans WHERE id = ?1",
            [scan_id],
            |row| row.get(0),
        )?;
        let entries_json = serde_json::to_string(&entries).map_err(|e| {
            StoreError::Db(rusqlite::Error::ToSqlConversionFailure(e.into()))
        })?;
        self.conn.execute(
            "INSERT INTO baselines(created_at_ms, scan_id, engine_version, entries_json)
             VALUES (?1, ?2, ?3, ?4)",
            params![now_ms, scan_id, engine_version, entries_json],
        )?;
        let baseline_id = self.conn.last_insert_rowid();
        self.conn.execute(
            "INSERT INTO active_baseline(key, baseline_id) VALUES (1, ?1)
             ON CONFLICT(key) DO UPDATE SET baseline_id = excluded.baseline_id",
            [baseline_id],
        )?;
        Ok(baseline_id)
    }

    pub fn current_baseline(&self) -> Result<Option<Baseline>> {
        let row: Option<(i64, String, String)> = self
            .conn
            .query_row(
                "SELECT b.id, b.engine_version, b.entries_json
                 FROM active_baseline a JOIN baselines b ON b.id = a.baseline_id
                 WHERE a.key = 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()?;
        match row {
            Some((_, engine_version, entries_json)) => {
                let entries: Vec<String> = serde_json::from_str(&entries_json).map_err(|e| {
                    StoreError::Db(rusqlite::Error::ToSqlConversionFailure(e.into()))
                })?;
                Ok(Some(Baseline {
                    engine_version,
                    entries,
                }))
            }
            None => Ok(None),
        }
    }

    /// 门禁差异：该扫描中不在当前基线内的发现。
    pub fn gate_new(&self, scan_id: i64) -> Result<Option<Vec<Finding>>> {
        match self.current_baseline()? {
            Some(baseline) => {
                let findings = self.get_findings(scan_id)?;
                Ok(Some(
                    findings
                        .into_iter()
                        .filter(|f| !baseline.contains(f))
                        .collect(),
                ))
            }
            None => Ok(None),
        }
    }
}

#[allow(dead_code)]
struct FindingRow {
    rule_id: String,
    dimension: String,
    severity: String,
    path: String,
    start_line: i64,
    start_col: i64,
    end_line: i64,
    end_col: i64,
    message: String,
    match_text: String,
    captures_json: String,
    degraded: i64,
    fingerprint: String,
}
