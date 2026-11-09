//! `codescan` 无头 CLI：把确定性扫描引擎接到 CI 门禁。
//!
//! 只做参数解析、引擎调用与结果序列化；所有扫描语义由 `codescan-engine` 提供。

use clap::{Args, Parser, Subcommand, ValueEnum};
use codescan_engine::baseline::Baseline;
use codescan_engine::scan::ScanOptions;
use codescan_engine::{sarif, CancelToken, Engine, ScanReport, Severity};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

#[derive(Debug, Parser)]
#[command(
    name = "codescan",
    version,
    about = "Codescan 确定性本地扫描引擎的无头 CLI"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// 扫描工作区并输出发现。
    Scan(ScanArgs),
}

#[derive(Debug, Args)]
struct ScanArgs {
    /// 工作区根目录。
    root: PathBuf,

    /// 输出格式。
    #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
    format: OutputFormat,

    /// 写入文件（默认 stdout）；给出时 stdout 只保留一行汇总。
    #[arg(long)]
    out: Option<PathBuf>,

    /// 自定义规则 YAML（叠加在内置规则之上）。
    #[arg(long)]
    rules: Option<PathBuf>,

    /// 额外全局忽略文件（gitignore 语法）。
    #[arg(long)]
    global_ignore: Option<PathBuf>,

    /// 读取基线（JSON），用于 `--fail-on new` 的差异门禁。
    #[arg(long)]
    baseline: Option<PathBuf>,

    /// 写入当前发现的基线快照（pretty JSON）。
    #[arg(long)]
    save_baseline: Option<PathBuf>,

    /// 门禁条件；命中时退出码为 1。
    #[arg(long, value_enum, default_value_t = FailOn::None)]
    fail_on: FailOn,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum OutputFormat {
    Text,
    Json,
    Sarif,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum FailOn {
    /// 任何不在基线中的新发现。
    New,
    /// 任一 error 级发现。
    Error,
    /// 任一 warning 或 error 级发现。
    Warning,
    /// 不因发现失败。
    None,
}

fn main() {
    let cli = Cli::parse();
    match run(cli) {
        Ok(code) => std::process::exit(code),
        Err(message) => {
            eprintln!("codescan: {message}");
            std::process::exit(2);
        }
    }
}

fn run(cli: Cli) -> Result<i32, String> {
    match cli.command {
        Command::Scan(args) => run_scan(args),
    }
}

fn run_scan(args: ScanArgs) -> Result<i32, String> {
    if !args.root.is_dir() {
        return Err(format!(
            "扫描根目录不存在或不是目录: {}",
            args.root.display()
        ));
    }

    let engine = build_engine(args.rules.as_deref())?;
    let baseline = load_baseline(args.baseline.as_deref())?;
    if args.fail_on == FailOn::New && baseline.is_none() {
        return Err("--fail-on new 需要同时提供 --baseline".into());
    }

    let options = ScanOptions {
        global_ignore_file: args.global_ignore.clone(),
    };
    let report = engine
        .scan_with_options(&args.root, &options, &CancelToken::new(), &|_| {})
        .map_err(|err| format!("扫描失败: {err}"))?;

    if let Some(path) = &args.save_baseline {
        let snapshot = Baseline::snapshot(&report);
        let content = serde_json::to_string_pretty(&snapshot)
            .map_err(|err| format!("基线序列化失败: {err}"))?;
        std::fs::write(path, format!("{content}\n"))
            .map_err(|err| format!("写入基线失败 {}: {err}", path.display()))?;
    }

    let summary = format!(
        "files={} findings={} events={}",
        report.stats.files_scanned,
        report.findings.len(),
        report.events.len()
    );
    let payload = render(&report, args.format, &summary)?;

    match &args.out {
        Some(path) => {
            std::fs::write(path, ensure_trailing_newline(payload))
                .map_err(|err| format!("写入输出文件失败 {}: {err}", path.display()))?;
            println!("{summary}");
        }
        None => {
            print!("{}", ensure_trailing_newline(payload));
        }
    }

    Ok(gate_exit_code(args.fail_on, baseline.as_ref(), &report))
}

fn build_engine(rules: Option<&Path>) -> Result<Engine, String> {
    let disabled = HashSet::new();
    let custom = match rules {
        Some(path) => {
            if !path.is_file() {
                return Err(format!("规则文件不存在: {}", path.display()));
            }
            let yaml = std::fs::read_to_string(path)
                .map_err(|err| format!("读取规则文件失败 {}: {err}", path.display()))?;
            Some(yaml)
        }
        None => None,
    };

    let result = match custom.as_deref() {
        Some(yaml) => Engine::with_rules(Some(yaml), &disabled),
        None => Engine::with_builtin(&disabled),
    };
    result.map_err(|errors| {
        let details: Vec<String> = errors.iter().map(|err| format!("  - {err}")).collect();
        format!("规则编译失败:\n{}", details.join("\n"))
    })
}

fn load_baseline(path: Option<&Path>) -> Result<Option<Baseline>, String> {
    let Some(path) = path else {
        return Ok(None);
    };
    if !path.is_file() {
        return Err(format!("基线文件不存在: {}", path.display()));
    }
    let raw = std::fs::read_to_string(path)
        .map_err(|err| format!("读取基线失败 {}: {err}", path.display()))?;
    let baseline = serde_json::from_str(&raw)
        .map_err(|err| format!("基线解析失败 {}: {err}", path.display()))?;
    Ok(Some(baseline))
}

fn render(report: &ScanReport, format: OutputFormat, summary: &str) -> Result<String, String> {
    match format {
        OutputFormat::Text => Ok(render_text(report, summary)),
        OutputFormat::Json => {
            serde_json::to_string_pretty(report).map_err(|err| format!("JSON 序列化失败: {err}"))
        }
        OutputFormat::Sarif => serde_json::to_string_pretty(&sarif::export(report))
            .map_err(|err| format!("SARIF 序列化失败: {err}")),
    }
}

fn render_text(report: &ScanReport, summary: &str) -> String {
    let mut out = String::new();
    out.push_str(summary);
    out.push('\n');
    for finding in &report.findings {
        out.push_str(&format!(
            "{}:{}:{} {} {} {}\n",
            finding.path,
            finding.span.start.line,
            finding.span.start.column,
            finding.rule_id,
            severity_label(finding.severity),
            finding.message
        ));
    }
    out
}

fn severity_label(severity: Severity) -> &'static str {
    match severity {
        Severity::Error => "error",
        Severity::Warning => "warning",
        Severity::Info => "info",
    }
}

fn ensure_trailing_newline(mut text: String) -> String {
    if !text.ends_with('\n') {
        text.push('\n');
    }
    text
}

fn gate_exit_code(fail_on: FailOn, baseline: Option<&Baseline>, report: &ScanReport) -> i32 {
    let hit = match fail_on {
        FailOn::None => false,
        FailOn::New => baseline.is_some_and(|baseline| !baseline.gate_new(report).is_empty()),
        FailOn::Error => report
            .findings
            .iter()
            .any(|finding| finding.severity == Severity::Error),
        FailOn::Warning => report
            .findings
            .iter()
            .any(|finding| finding.severity != Severity::Info),
    };
    if hit {
        1
    } else {
        0
    }
}
