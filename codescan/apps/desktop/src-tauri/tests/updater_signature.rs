//! 发布验收（默认跳过，`--ignored` 运行）：updater 工件 minisign 签名验证。
//!
//! 用 minisign-verify（tauri-plugin-updater 应用内更新时的同一验证实现）对
//! `task build:dmg` 产出的 `Codescan.app.tar.gz(.sig)` 验证 tauri.release.conf.json
//! 提交的公钥。工件缺失时静默跳过（默认 `cargo test` 无须产物在场）。
//!
//! 运行：`cargo test -p codescan-desktop --test updater_signature -- --ignored`

use base64::Engine as _;
use minisign_verify::{PublicKey, Signature};
use std::path::{Path, PathBuf};

/// tauri signer 产物是 base64 包裹形态（单行），归一成原始 minisign 文本；
/// 已是原始文本（多行）时原样返回。
fn unwrap_base64_wrapped(text: &str) -> String {
    let trimmed = text.trim();
    if let Ok(decoded) = base64::engine::general_purpose::STANDARD.decode(trimmed) {
        if let Ok(s) = String::from_utf8(decoded) {
            return s;
        }
    }
    trimmed.to_string()
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .canonicalize()
        .expect("定位仓库根")
}

#[test]
#[ignore = "发布验收：依赖本地已构建的 updater 工件（task build:dmg 后运行）"]
fn updater_artifact_signature_matches_committed_pubkey() {
    let root = repo_root();
    let conf_path = root.join("apps/desktop/src-tauri/tauri.release.conf.json");
    let tarball = root.join("target/aarch64-apple-darwin/release/bundle/macos/Codescan.app.tar.gz");
    let sig_file = root.join("target/aarch64-apple-darwin/release/bundle/macos/Codescan.app.tar.gz.sig");
    for path in [&conf_path, &tarball, &sig_file] {
        if !Path::new(path).exists() {
            eprintln!("skip：{} 不存在（未构建 updater 工件）", path.display());
            return;
        }
    }

    let conf: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(&conf_path).expect("读取 tauri.release.conf.json"),
    )
    .expect("解析 tauri.release.conf.json");
    let pubkey_wrapped = conf
        .pointer("/plugins/updater/pubkey")
        .and_then(|v| v.as_str())
        .expect("tauri.release.conf.json 缺 plugins.updater.pubkey");

    // 公钥文本 = 注释行 + keynum||key 数据行；PublicKey 取数据行
    let pubkey_text = unwrap_base64_wrapped(pubkey_wrapped);
    let key_line = pubkey_text
        .lines()
        .find(|l| !l.starts_with("untrusted comment:"))
        .expect("公钥文本缺数据行");
    let public_key = PublicKey::from_base64(key_line.trim()).expect("公钥解析失败");

    let sig_text = unwrap_base64_wrapped(
        &std::fs::read_to_string(&sig_file).expect("读取 .sig"),
    );
    let signature = Signature::decode(&sig_text).expect("签名解析失败");
    let artifact = std::fs::read(&tarball).expect("读取更新工件");

    // tauri signer 产出的签名带 prehash 算法标记，走 verify 正路（legacy=false）
    public_key
        .verify(&artifact, &signature, false)
        .expect("updater 工件签名与提交公钥不匹配");
}
