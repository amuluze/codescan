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
    /// 上传 SARIF 到 GitHub Code Scanning（REST API）。
    GithubUpload(GithubUploadArgs),
}

#[derive(Debug, Args)]
struct GithubUploadArgs {
    /// SARIF 2.1.0 文件路径。
    #[arg(long)]
    sarif: PathBuf,

    /// owner/repo（默认 GITHUB_REPOSITORY）。
    #[arg(long)]
    repo: Option<String>,

    /// 完整 40 位 commit SHA（默认 GITHUB_SHA）。
    #[arg(long)]
    commit: Option<String>,

    /// Git ref（默认 GITHUB_REF，再回退 refs/heads/main）。
    #[arg(long = "ref")]
    git_ref: Option<String>,

    /// 仅输出请求元数据，不发起网络请求。
    #[arg(long)]
    dry_run: bool,
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
        Command::GithubUpload(args) => run_github_upload(args),
    }
}

// ---------- GitHub Code Scanning 上传（v6.1） ----------

fn run_github_upload(args: GithubUploadArgs) -> Result<i32, String> {
    let sarif_bytes = std::fs::read(&args.sarif)
        .map_err(|err| format!("读取 SARIF 失败 {}: {err}", args.sarif.display()))?;
    let value: serde_json::Value =
        serde_json::from_slice(&sarif_bytes).map_err(|err| format!("SARIF 解析失败: {err}"))?;
    if value.get("version").and_then(|v| v.as_str()) != Some("2.1.0") {
        return Err("SARIF version 必须为 2.1.0".into());
    }

    let repo = args
        .repo
        .or_else(|| std::env::var("GITHUB_REPOSITORY").ok())
        .ok_or_else(|| "缺少 --repo 或 GITHUB_REPOSITORY".to_string())?;
    validate_repo(&repo)?;
    let commit = args
        .commit
        .or_else(|| std::env::var("GITHUB_SHA").ok())
        .ok_or_else(|| "缺少 --commit 或 GITHUB_SHA".to_string())?;
    validate_commit_sha(&commit)?;
    let git_ref = args
        .git_ref
        .or_else(|| std::env::var("GITHUB_REF").ok())
        .unwrap_or_else(|| "refs/heads/main".to_string());
    if git_ref.trim().is_empty() {
        return Err("ref 不能为空".into());
    }

    let payload = encode_sarif(&sarif_bytes)?;
    let endpoint = format!("https://api.github.com/repos/{repo}/code-scanning/sarifs");
    let token_present = std::env::var("GITHUB_TOKEN")
        .map(|t| !t.trim().is_empty())
        .unwrap_or(false);

    if args.dry_run {
        println!("endpoint={endpoint}");
        println!("commit_sha={commit}");
        println!("ref={git_ref}");
        println!("sarif_bytes={}", sarif_bytes.len());
        println!("payload_bytes={}", payload.len());
        println!("token_present={token_present}");
        return Ok(0);
    }

    let token = std::env::var("GITHUB_TOKEN")
        .ok()
        .filter(|t| !t.trim().is_empty())
        .ok_or_else(|| "未配置 GITHUB_TOKEN（需要 security-events: write 权限）".to_string())?;
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(60))
        .user_agent("codescan-cli")
        .build()
        .map_err(|err| format!("HTTP 客户端创建失败: {err}"))?;
    let response = client
        .post(&endpoint)
        .bearer_auth(token)
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .json(&serde_json::json!({
            "commit_sha": commit,
            "ref": git_ref,
            "sarif": payload,
            "tool_name": "Codescan",
        }))
        .send()
        .map_err(|err| format!("上传请求失败: {err}"))?;
    let status = response.status();
    let body = response.text().unwrap_or_default();
    if !status.is_success() {
        return Err(format!(
            "GitHub 返回 {status}: {}",
            truncate_text(&body, 500)
        ));
    }
    let parsed: serde_json::Value = serde_json::from_str(&body).unwrap_or(serde_json::Value::Null);
    let id = parsed.get("id").and_then(|v| v.as_str()).unwrap_or("");
    let url = parsed.get("url").and_then(|v| v.as_str()).unwrap_or("");
    if id.is_empty() {
        println!("uploaded (no id in response)");
    } else {
        println!("uploaded id={id} url={url}");
    }
    Ok(0)
}

fn validate_repo(repo: &str) -> Result<(), String> {
    let valid = repo.split_once('/').is_some_and(|(owner, name)| {
        !owner.is_empty()
            && !name.is_empty()
            && !name.contains('/')
            && owner
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
            && name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
    });
    if valid {
        Ok(())
    } else {
        Err(format!("非法 repo（期望 owner/name）: {repo}"))
    }
}

fn validate_commit_sha(sha: &str) -> Result<(), String> {
    if sha.len() == 40 && sha.chars().all(|c| c.is_ascii_hexdigit()) {
        Ok(())
    } else {
        Err("commit SHA 必须是 40 位十六进制".into())
    }
}

/// gzip + base64（GitHub SARIF 上传要求）。
fn encode_sarif(bytes: &[u8]) -> Result<String, String> {
    use base64::Engine as _;
    use std::io::Write as _;
    let mut encoder = flate2::write::GzEncoder::new(
        Vec::new(),
        flate2::Compression::default(),
    );
    encoder
        .write_all(bytes)
        .map_err(|err| format!("gzip 压缩失败: {err}"))?;
    let compressed = encoder
        .finish()
        .map_err(|err| format!("gzip 压缩失败: {err}"))?;
    Ok(base64::engine::general_purpose::STANDARD.encode(compressed))
}

fn truncate_text(text: &str, max: usize) -> String {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sarif_encoding_roundtrip() {
        let raw = br#"{"version":"2.1.0","runs":[]}"#;
        let encoded = encode_sarif(raw).expect("encode");
        use base64::Engine as _;
        use std::io::Read as _;
        let compressed = base64::engine::general_purpose::STANDARD
            .decode(encoded)
            .expect("base64");
        let mut decoder = flate2::read::GzDecoder::new(&compressed[..]);
        let mut decoded = Vec::new();
        decoder.read_to_end(&mut decoded).expect("gunzip");
        assert_eq!(decoded, raw);
    }

    #[test]
    fn repo_and_commit_validation() {
        assert!(validate_repo("owner/repo").is_ok());
        assert!(validate_repo("owner/repo.name").is_ok());
        assert!(validate_repo("owner").is_err());
        assert!(validate_repo("owner/repo/extra").is_err());
        assert!(validate_repo("owner//repo").is_err());
        assert!(validate_repo("own er/repo").is_err());

        assert!(validate_commit_sha(&"a".repeat(40)).is_ok());
        assert!(validate_commit_sha(&"a".repeat(39)).is_err());
        assert!(validate_commit_sha(&"g".repeat(40)).is_err());
    }

    #[test]
    fn truncation_respects_utf8_boundaries() {
        let text = "中文中文中文";
        let out = truncate_text(text, 7);
        assert!(out.ends_with('…'));
        assert!(out.len() <= 10);
    }
}
