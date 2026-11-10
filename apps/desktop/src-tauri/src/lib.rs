//! Codescan 桌面应用壳：业务在 codescan-engine 与 store/manager（均无头可测）。

pub mod commands;
pub mod llm;
pub mod manager;
pub mod regression;
pub mod store;

use commands::AppState;
use std::sync::{Arc, Mutex};

pub fn run() {
    let data_root = {
        let home = std::env::var_os("HOME")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| std::path::PathBuf::from("."));
        home.join(".codescan")
    };
    let _ = std::fs::create_dir_all(&data_root);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&data_root, std::fs::Permissions::from_mode(0o700));
    }
    let db_path = data_root.join("codescan.db");
    let store = store::Store::open(&db_path).expect("打开 Codescan 数据库失败");
    let store = Arc::new(Mutex::new(store));

    let context = tauri::generate_context!();
    let builder = tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            commands::list_directory,
            commands::list_rules,
            commands::set_rule_enabled,
            commands::custom_rules_status,
            commands::start_scan,
            commands::cancel_scan,
            commands::list_scans,
            commands::get_scan,
            commands::delete_scan,
            commands::set_baseline,
            commands::gate_diff,
            commands::baseline_info,
            commands::export_sarif,
            commands::set_provider_config,
            commands::get_provider_config,
            commands::set_api_key,
            commands::clear_api_key,
            commands::test_provider_connection,
            commands::llm_review_workspace,
            commands::llm_triage,
            commands::llm_triage_stream,
            commands::llm_batch_triage,
            commands::llm_generate_rule,
            commands::llm_candidate_regression,
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
        .expect("error while running codescan desktop");
}
