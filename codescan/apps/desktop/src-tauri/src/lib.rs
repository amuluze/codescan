//! Codescan 桌面应用壳：业务在 codescan-engine 与 store/manager（均无头可测）。

pub mod commands;
pub mod llm;
pub mod manager;
pub mod regression;
pub mod store;

use commands::AppState;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard};

/// 桌面应用壳入口：初始化失败（数据目录、数据库、事件循环）返回可读错误，
/// 由 `main` 打印并以非零码退出，启动路径上不 panic。
pub fn run() -> Result<(), String> {
    let data_root = data_root();
    prepare_data_root(&data_root).map_err(|error| format!("初始化数据目录失败：{error}"))?;
    let db_path = data_root.join("codescan.db");
    let store = store::Store::open(&db_path)
        .map_err(|error| format!("打开 Codescan 数据库失败：{error}"))?;
    let store = Arc::new(Mutex::new(store));
    let context = tauri::generate_context!();
    let builder = tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            commands::list_directory,
            commands::list_rules,
            commands::set_rule_enabled,
            commands::custom_rules_status,
            commands::get_custom_rules,
            commands::list_rule_packs,
            commands::install_rule_pack,
            commands::remove_rule_pack,
            commands::validate_custom_rules,
            commands::save_custom_rules,
            commands::test_custom_rule,
            commands::start_scan,
            commands::cancel_scan,
            commands::list_scans,
            commands::get_scan,
            commands::get_scan_events,
            commands::get_scan_ruleset,
            commands::delete_scan,
            commands::set_baseline,
            commands::gate_diff,
            commands::baseline_info,
            commands::export_sarif,
            commands::export_report,
            commands::list_providers,
            commands::upsert_provider_profile,
            commands::delete_provider_profile,
            commands::set_active_provider,
            commands::test_provider_connection,
            commands::llm_review_workspace,
            commands::llm_triage,
            commands::llm_triage_stream,
            commands::llm_batch_triage,
            commands::llm_generate_rule,
            commands::llm_candidate_regression,
            commands::llm_review_skill_info,
            commands::save_llm_review_skill,
            commands::reset_llm_review_skill,
            commands::save_custom_rule,
        ])
        .manage(AppState {
            manager: manager::ScanManager::new(store.clone())
                .with_global_ignore(Some(data_root.join("ignore"))),
            store,
        });
    // 自更新插件按需注册：updater 的 Config 反序列化不接受 null，配置缺失
    // （开发模式 / 无签名凭据构建，见 scripts/generate-release-config.mjs）时
    // 注册会在 PluginInitialization 阶段失败并阻断启动，故仅在配置存在时挂载。
    let builder = if context.config().plugins.0.contains_key("updater") {
        builder.plugin(tauri_plugin_updater::Builder::new().build())
    } else {
        builder
    };
    builder
        .plugin(tauri_plugin_process::init())
        .run(context)
        .map_err(|error| format!("Codescan 桌面端运行失败：{error}"))
}

/// 数据根目录 `~/.codescan`（HOME 缺失时退回当前目录）。
pub(crate) fn data_root() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".codescan")
}

/// 建数据根目录，并在 Unix 上收紧为仅属主可访问（目录内存放含密钥的 SQLite）。
/// 权限收紧属纵深防御：不支持 Unix 权限的卷不应阻止启动，失败时留下可排查的告警。
fn prepare_data_root(data_root: &std::path::Path) -> std::io::Result<()> {
    std::fs::create_dir_all(data_root)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Err(error) =
            std::fs::set_permissions(data_root, std::fs::Permissions::from_mode(0o700))
        {
            eprintln!(
                "codescan: 警告：无法将数据目录 {} 收紧为 0700：{error}",
                data_root.display()
            );
        }
    }
    Ok(())
}

/// 取锁：中毒只说明持锁线程曾 panic，被保护的数据（扫描登记表 / SQLite 连接）不含
/// 需要回滚的不变量，取回继续使用，避免一次 panic 让后续命令连锁 panic。
pub(crate) fn lock_recovering<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}
