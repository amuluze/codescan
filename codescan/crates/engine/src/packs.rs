//! 本地规则包 MVP：manifest 校验、安装/列出/卸载与规则合并。
//! 包目录：<data_root>/packs/<id>/<version>/；签名/远程更新不在本层。

use crate::Engine;
use base64::Engine as _;
use ring::rand::SystemRandom;
use ring::signature::{Ed25519KeyPair, KeyPair, UnparsedPublicKey, ED25519};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::path::{Component, Path, PathBuf};

const TRUSTED_KEYS_DIR: &str = "trusted-keys";
const SIGNATURE_FILE: &str = ".signature";
const SIGNATURE_ALGORITHM: &str = "ed25519";
/// 规则包内文件（pack.yaml / 规则文件 / 签名 / 完整性标记）与自定义规则文件的读取上限。
const MAX_RULE_FILE_BYTES: u64 = 1024 * 1024;
/// 信任公钥文件的读取上限（Ed25519 公钥为 32 字节）。
const MAX_KEY_FILE_BYTES: u64 = 64 * 1024;

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PackManifest {
    pub id: String,
    pub version: String,
    #[serde(default)]
    pub engine_min: Option<String>,
    #[serde(default)]
    pub engine_max: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    pub rules: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PackSnapshot {
    pub id: String,
    pub version: String,
    pub integrity: String,
    pub signed_by: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RulePackInfo {
    pub id: String,
    pub version: String,
    pub engine_min: Option<String>,
    pub engine_max: Option<String>,
    pub description: Option<String>,
    pub rules: usize,
    pub path: String,
    pub integrity: String,
    pub signed_by: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PackSignature {
    pub algorithm: String,
    pub key_id: String,
    pub signature: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct TrustedKeyInfo {
    pub key_id: String,
    pub path: String,
}

struct TrustedKey {
    info: TrustedKeyInfo,
    public_key: Vec<u8>,
}

fn packs_root(data_root: &Path) -> PathBuf {
    data_root.join("packs")
}

fn pack_dir(data_root: &Path, id: &str, version: &str) -> PathBuf {
    packs_root(data_root).join(id).join(version)
}

fn valid_component(value: &str) -> bool {
    !value.is_empty()
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
        && !value.starts_with('.')
}

fn valid_relative_rule_path(value: &str) -> bool {
    let path = Path::new(value);
    !path.is_absolute()
        && path.components().all(|component| matches!(component, Component::Normal(_)))
        && matches!(
            path.extension().and_then(|ext| ext.to_str()),
            Some("yaml" | "yml")
        )
}

fn version_parts(value: &str) -> Vec<u64> {
    value
        .split('.')
        .map(|part| {
            part.chars()
                .take_while(|c| c.is_ascii_digit())
                .collect::<String>()
                .parse()
                .unwrap_or(0)
        })
        .collect()
}

fn version_cmp(left: &str, right: &str) -> std::cmp::Ordering {
    let mut left = version_parts(left);
    let mut right = version_parts(right);
    let len = left.len().max(right.len());
    left.resize(len, 0);
    right.resize(len, 0);
    left.cmp(&right)
}

fn validate_manifest(manifest: &PackManifest) -> Result<(), String> {
    if !valid_component(&manifest.id) {
        return Err(format!("非法规则包 id：{}", manifest.id));
    }
    if !valid_component(&manifest.version) {
        return Err(format!("非法规则包版本：{}", manifest.version));
    }
    if manifest.rules.is_empty() {
        return Err("规则包至少需要一个规则文件".into());
    }
    for rule in &manifest.rules {
        if !valid_relative_rule_path(rule) {
            return Err(format!("非法规则文件路径：{rule}"));
        }
    }
    let current = env!("CARGO_PKG_VERSION");
    if let Some(min) = &manifest.engine_min {
        if version_cmp(current, min) == std::cmp::Ordering::Less {
            return Err(format!("规则包要求引擎 ≥ {min}，当前 {current}"));
        }
    }
    if let Some(max) = &manifest.engine_max {
        if version_cmp(current, max) == std::cmp::Ordering::Greater {
            return Err(format!("规则包要求引擎 ≤ {max}，当前 {current}"));
        }
    }
    Ok(())
}

fn read_manifest(dir: &Path) -> Result<PackManifest, String> {
    let path = dir.join("pack.yaml");
    let text = crate::fsutil::read_to_string_limited(&path, MAX_RULE_FILE_BYTES)
        .map_err(|error| format!("读取 {} 失败：{error}", path.display()))?;
    let manifest: PackManifest = serde_yaml::from_str(&text)
        .map_err(|error| format!("解析 {} 失败：{error}", path.display()))?;
    validate_manifest(&manifest)?;
    Ok(manifest)
}

fn read_rule_values(dir: &Path, manifest: &PackManifest) -> Result<Vec<serde_yaml::Value>, String> {
    let mut rules = Vec::new();
    for relative in &manifest.rules {
        let path = dir.join(relative);
        let text = crate::fsutil::read_to_string_limited(&path, MAX_RULE_FILE_BYTES)
            .map_err(|error| format!("读取 {} 失败：{error}", path.display()))?;
        let value: serde_yaml::Value = serde_yaml::from_str(&text)
            .map_err(|error| format!("解析 {} 失败：{error}", path.display()))?;
        let Some(items) = value
            .get("rules")
            .and_then(|value| value.as_sequence())
        else {
            return Err(format!("{} 缺少顶层 rules 序列", path.display()));
        };
        rules.extend(items.iter().cloned());
    }
    if rules.is_empty() {
        return Err("规则包没有有效规则".into());
    }
    Ok(rules)
}

fn pack_content_hash(dir: &Path, manifest: &PackManifest) -> Result<String, String> {
    let mut content = crate::fsutil::read_to_string_limited(&dir.join("pack.yaml"), MAX_RULE_FILE_BYTES)
        .map_err(|error| format!("读取 pack.yaml 失败：{error}"))?;
    for relative in &manifest.rules {
        content.push('\0');
        content.push_str(
            &crate::fsutil::read_to_string_limited(&dir.join(relative), MAX_RULE_FILE_BYTES)
                .map_err(|error| format!("读取 {relative} 失败：{error}"))?,
        );
    }
    Ok(crate::fingerprint::content_hash(&content))
}

fn read_integrity(dir: &Path) -> Option<String> {
    crate::fsutil::read_to_string_limited(&dir.join(".integrity"), MAX_RULE_FILE_BYTES)
        .ok()
        .map(|value| value.trim().to_string())
}

fn verify_integrity(dir: &Path, manifest: &PackManifest) -> Result<String, String> {
    let hash = pack_content_hash(dir, manifest)?;
    match read_integrity(dir) {
        Some(stored) if stored == hash => Ok(hash),
        Some(_) => Err("规则包内容完整性校验失败".into()),
        None => Err("规则包缺少完整性标记".into()),
    }
}

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn decode_base64(value: &str) -> Result<Vec<u8>, String> {
    let text = value.trim();
    base64::engine::general_purpose::STANDARD
        .decode(text)
        .or_else(|_| base64::engine::general_purpose::STANDARD_NO_PAD.decode(text))
        .map_err(|error| format!("base64 解码失败：{error}"))
}

pub fn public_key_id(public_key: &[u8]) -> Result<String, String> {
    if public_key.len() != 32 {
        return Err("Ed25519 公钥必须为 32 字节".into());
    }
    Ok(hex_encode(&Sha256::digest(public_key)))
}

pub fn parse_public_key(value: &str) -> Result<Vec<u8>, String> {
    let bytes = decode_base64(value)?;
    if bytes.len() != 32 {
        return Err("Ed25519 公钥必须为 32 字节".into());
    }
    Ok(bytes)
}

fn parse_private_key(value: &str) -> Result<Vec<u8>, String> {
    let bytes = decode_base64(value)?;
    if bytes.is_empty() {
        return Err("私钥为空".into());
    }
    Ok(bytes)
}

fn signature_message(hash: &str) -> String {
    format!("codescan-pack-integrity:v1:{hash}")
}

fn trusted_keys_root(data_root: &Path) -> PathBuf {
    data_root.join(TRUSTED_KEYS_DIR)
}

fn load_trusted_keys(data_root: &Path) -> Result<Vec<TrustedKey>, String> {
    let root = trusted_keys_root(data_root);
    let entries = match std::fs::read_dir(&root) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => {
            return Err(format!("读取信任公钥目录失败 {}：{error}", root.display()));
        }
    };
    let mut keys = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|error| format!("读取信任公钥目录失败：{error}"))?;
        if !entry.file_type().map(|kind| kind.is_file()).unwrap_or(false)
            || entry.path().extension().and_then(|ext| ext.to_str()) != Some("pub")
        {
            continue;
        }
        let public_key = crate::fsutil::read_bytes_limited(&entry.path(), MAX_KEY_FILE_BYTES).map_err(|error| {
            format!("读取信任公钥失败 {}：{error}", entry.path().display())
        })?;
        let key_id = public_key_id(&public_key).map_err(|error| {
            format!("信任公钥无效 {}：{error}", entry.path().display())
        })?;
        keys.push(TrustedKey {
            info: TrustedKeyInfo {
                key_id,
                path: entry.path().to_string_lossy().into_owned(),
            },
            public_key,
        });
    }
    keys.sort_by(|a, b| a.info.key_id.cmp(&b.info.key_id));
    Ok(keys)
}

pub fn trusted_keys(data_root: &Path) -> Result<Vec<TrustedKeyInfo>, String> {
    Ok(load_trusted_keys(data_root)?
        .into_iter()
        .map(|key| key.info)
        .collect())
}

pub fn trust_key(data_root: &Path, public_key: &[u8]) -> Result<TrustedKeyInfo, String> {
    let key_id = public_key_id(public_key)?;
    let root = trusted_keys_root(data_root);
    std::fs::create_dir_all(&root).map_err(|error| error.to_string())?;
    let path = root.join(format!("{key_id}.pub"));
    std::fs::write(&path, public_key).map_err(|error| error.to_string())?;
    Ok(TrustedKeyInfo {
        key_id,
        path: path.to_string_lossy().into_owned(),
    })
}

fn valid_key_id(key_id: &str) -> bool {
    key_id.len() == 64 && key_id.chars().all(|c| c.is_ascii_hexdigit())
}

pub fn untrust_key(data_root: &Path, key_id: &str) -> Result<(), String> {
    if !valid_key_id(key_id) {
        return Err("非法公钥 id".into());
    }
    let path = trusted_keys_root(data_root).join(format!("{key_id}.pub"));
    if !path.is_file() {
        return Err(format!("信任公钥不存在：{key_id}"));
    }
    std::fs::remove_file(path).map_err(|error| error.to_string())
}

fn read_signature(dir: &Path) -> Result<Option<PackSignature>, String> {
    let path = dir.join(SIGNATURE_FILE);
    if !path.is_file() {
        return Ok(None);
    }
    let text =
        crate::fsutil::read_to_string_limited(&path, MAX_RULE_FILE_BYTES)
            .map_err(|error| format!("读取签名失败：{error}"))?;
    let signature: PackSignature =
        serde_json::from_str(&text).map_err(|error| format!("签名文件解析失败：{error}"))?;
    if signature.algorithm != SIGNATURE_ALGORITHM {
        return Err(format!("不支持的签名算法：{}", signature.algorithm));
    }
    if !valid_key_id(&signature.key_id) {
        return Err("签名公钥 id 非法".into());
    }
    let bytes = decode_base64(&signature.signature)?;
    if bytes.len() != 64 {
        return Err("Ed25519 签名必须为 64 字节".into());
    }
    Ok(Some(signature))
}

fn verify_signature(
    dir: &Path,
    hash: &str,
    keys: &[TrustedKey],
) -> Result<Option<String>, String> {
    let Some(signature) = read_signature(dir)? else {
        if keys.is_empty() {
            return Ok(None);
        }
        return Err("规则包未签名，但已配置信任公钥".into());
    };
    if keys.is_empty() {
        return Err("规则包已签名，但未配置信任公钥".into());
    }
    let key = keys
        .iter()
        .find(|key| key.info.key_id == signature.key_id)
        .ok_or_else(|| format!("规则包签名密钥未受信任：{}", signature.key_id))?;
    let signature_bytes = decode_base64(&signature.signature)?;
    UnparsedPublicKey::new(&ED25519, &key.public_key)
        .verify(signature_message(hash).as_bytes(), &signature_bytes)
        .map_err(|_| "规则包签名校验失败".to_string())?;
    Ok(Some(signature.key_id))
}

fn signature_of_keypair(
    keypair: &Ed25519KeyPair,
    hash: &str,
) -> Result<PackSignature, String> {
    Ok(PackSignature {
        algorithm: SIGNATURE_ALGORITHM.into(),
        key_id: public_key_id(keypair.public_key().as_ref())?,
        signature: base64::engine::general_purpose::STANDARD
            .encode(keypair.sign(signature_message(hash).as_bytes()).as_ref()),
    })
}

pub fn generate_keypair() -> Result<(String, String), String> {
    let rng = SystemRandom::new();
    let document = Ed25519KeyPair::generate_pkcs8(&rng)
        .map_err(|_| "Ed25519 密钥生成失败".to_string())?;
    let keypair = Ed25519KeyPair::from_pkcs8(document.as_ref())
        .map_err(|error| format!("Ed25519 私钥解析失败：{error}"))?;
    Ok((
        base64::engine::general_purpose::STANDARD.encode(document.as_ref()),
        base64::engine::general_purpose::STANDARD.encode(keypair.public_key().as_ref()),
    ))
}

pub fn sign_pack(dir: &Path, private_key: &str) -> Result<PackSignature, String> {
    let manifest = read_manifest(dir)?;
    let _ = read_rule_values(dir, &manifest)?;
    let hash = pack_content_hash(dir, &manifest)?;
    let key_bytes = parse_private_key(private_key)?;
    let keypair = Ed25519KeyPair::from_pkcs8(&key_bytes)
        .map_err(|error| format!("Ed25519 私钥解析失败：{error}"))?;
    let signature = signature_of_keypair(&keypair, &hash)?;
    let text = serde_json::to_string_pretty(&signature)
        .map_err(|error| format!("签名序列化失败：{error}"))?;
    std::fs::write(dir.join(SIGNATURE_FILE), format!("{text}\n"))
        .map_err(|error| format!("写入签名失败：{error}"))?;
    Ok(signature)
}

fn serialize_rules(rules: Vec<serde_yaml::Value>) -> Result<String, String> {
    let mut document = serde_yaml::Mapping::new();
    document.insert(
        serde_yaml::Value::String("rules".into()),
        serde_yaml::Value::Sequence(rules),
    );
    serde_yaml::to_string(&serde_yaml::Value::Mapping(document))
        .map_err(|error| format!("序列化规则包失败：{error}"))
}

pub fn list(data_root: &Path) -> Vec<RulePackInfo> {
    let mut infos = Vec::new();
    let keys = load_trusted_keys(data_root);
    let key_error = keys.as_ref().err().cloned();
    let keys = keys.unwrap_or_default();
    let Ok(ids) = std::fs::read_dir(packs_root(data_root)) else {
        return infos;
    };
    for id in ids.flatten() {
        if !id.file_type().map(|kind| kind.is_dir()).unwrap_or(false) {
            continue;
        }
        let Ok(versions) = std::fs::read_dir(id.path()) else {
            continue;
        };
        for version in versions.flatten() {
            if !version.file_type().map(|kind| kind.is_dir()).unwrap_or(false) {
                continue;
            }
            let dir = version.path();
            match read_manifest(&dir) {
                Ok(manifest) => {
                    let integrity = verify_integrity(&dir, &manifest);
                    let (signed_by, error) = if let Some(error) = &key_error {
                        (None, Some(error.clone()))
                    } else {
                        match &integrity {
                            Ok(hash) => match verify_signature(&dir, hash, &keys) {
                                Ok(signed_by) => (signed_by, None),
                                Err(error) => (None, Some(error)),
                            },
                            Err(error) => (None, Some(error.clone())),
                        }
                    };
                    infos.push(RulePackInfo {
                        id: manifest.id,
                        version: manifest.version,
                        engine_min: manifest.engine_min,
                        engine_max: manifest.engine_max,
                        description: manifest.description,
                        rules: manifest.rules.len(),
                        path: dir.to_string_lossy().into_owned(),
                        integrity: integrity.unwrap_or_default(),
                        signed_by,
                        error,
                    });
                }
                Err(error) => infos.push(RulePackInfo {
                    id: id.file_name().to_string_lossy().into_owned(),
                    version: version.file_name().to_string_lossy().into_owned(),
                    engine_min: None,
                    engine_max: None,
                    description: None,
                    rules: 0,
                    path: dir.to_string_lossy().into_owned(),
                    integrity: String::new(),
                    signed_by: None,
                    error: Some(error),
                }),
            }
        }
    }
    infos.sort_by(|a, b| (&a.id, &a.version).cmp(&(&b.id, &b.version)));
    infos
}

fn installed_pack_rules(data_root: &Path) -> Result<Vec<serde_yaml::Value>, String> {
    let mut rules = Vec::new();
    for info in list(data_root) {
        if let Some(error) = info.error {
            return Err(format!("已安装规则包 {}@{} 无效：{error}", info.id, info.version));
        }
        let dir = PathBuf::from(&info.path);
        let manifest = read_manifest(&dir)?;
        let mut values = read_rule_values(&dir, &manifest)?;
        rules.append(&mut values);
    }
    Ok(rules)
}

fn custom_rule_values(data_root: &Path) -> Result<Vec<serde_yaml::Value>, String> {
    let path = data_root.join("rules").join("custom.yaml");
    if !path.is_file() {
        return Ok(Vec::new());
    }
    let text = crate::fsutil::read_to_string_limited(&path, MAX_RULE_FILE_BYTES)
        .map_err(|error| format!("读取 {} 失败：{error}", path.display()))?;
    let value: serde_yaml::Value =
        serde_yaml::from_str(&text).map_err(|error| format!("自定义规则解析失败：{error}"))?;
    Ok(value
        .get("rules")
        .and_then(|value| value.as_sequence())
        .map(|items| items.to_vec())
        .unwrap_or_default())
}

pub fn snapshots(data_root: &Path) -> Vec<PackSnapshot> {
    list(data_root)
        .into_iter()
        .filter(|info| info.error.is_none())
        .map(|info| PackSnapshot {
            id: info.id,
            version: info.version,
            integrity: info.integrity,
            signed_by: info.signed_by,
        })
        .collect()
}

pub fn install(data_root: &Path, source: &Path) -> Result<RulePackInfo, String> {
    if !source.is_dir() {
        return Err(format!("规则包目录不存在：{}", source.display()));
    }
    let manifest = read_manifest(source)?;
    let rules = read_rule_values(source, &manifest)?;
    let keys = load_trusted_keys(data_root)?;
    let source_hash = pack_content_hash(source, &manifest)?;
    verify_signature(source, &source_hash, &keys)?;
    let mut all_rules = installed_pack_rules(data_root)?;
    all_rules.extend(rules);
    all_rules.extend(custom_rule_values(data_root)?);
    let combined = serialize_rules(all_rules)?;
    // 校验完整规则集：内置 + 既有包 + 新包 + 自定义。
    Engine::with_rules(Some(&combined), &HashSet::new())
        .map_err(|errors| errors.iter().map(|e| e.to_string()).collect::<Vec<_>>().join("\n"))?;

    let target = pack_dir(data_root, &manifest.id, &manifest.version);
    if target.exists() {
        return Err(format!(
            "规则包已安装：{}@{}",
            manifest.id, manifest.version
        ));
    }
    let root = packs_root(data_root);
    std::fs::create_dir_all(&root).map_err(|error| error.to_string())?;
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|value| value.as_nanos())
        .unwrap_or_default();
    let temp = root.join(format!(".install-{}-{unique}", std::process::id()));
    std::fs::create_dir_all(&temp).map_err(|error| error.to_string())?;

    let result = (|| -> Result<Option<String>, String> {
        std::fs::copy(source.join("pack.yaml"), temp.join("pack.yaml"))
            .map_err(|error| error.to_string())?;
        for relative in &manifest.rules {
            let destination = temp.join(relative);
            if let Some(parent) = destination.parent() {
                std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
            }
            std::fs::copy(source.join(relative), destination)
                .map_err(|error| error.to_string())?;
        }
        let signature = source.join(SIGNATURE_FILE);
        if signature.is_file() {
            std::fs::copy(&signature, temp.join(SIGNATURE_FILE))
                .map_err(|error| error.to_string())?;
        }
        let integrity = pack_content_hash(&temp, &manifest)?;
        let signed_by = verify_signature(&temp, &integrity, &keys)?;
        std::fs::write(temp.join(".integrity"), &integrity).map_err(|error| error.to_string())?;
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        std::fs::rename(&temp, &target).map_err(|error| error.to_string())?;
        Ok(signed_by)
    })();
    let signed_by = match result {
        Ok(signed_by) => signed_by,
        Err(error) => {
            // 安装失败后清理临时目录；清理失败只留下磁盘残留，附加到原错误便于排查。
            let cleanup = std::fs::remove_dir_all(&temp)
                .err()
                .map(|cleanup_error| format!("；清理临时目录 {} 失败：{cleanup_error}", temp.display()))
                .unwrap_or_default();
            return Err(format!("{error}{cleanup}"));
        }
    };
    Ok(RulePackInfo {
        id: manifest.id,
        version: manifest.version,
        engine_min: manifest.engine_min,
        engine_max: manifest.engine_max,
        description: manifest.description,
        rules: manifest.rules.len(),
        path: target.to_string_lossy().into_owned(),
        integrity: read_integrity(&target).unwrap_or_default(),
        signed_by,
        error: None,
    })
}

pub fn remove(data_root: &Path, id: &str, version: &str) -> Result<(), String> {
    if !valid_component(id) || !valid_component(version) {
        return Err("非法规则包 id 或版本".into());
    }
    let target = pack_dir(data_root, id, version);
    if !target.exists() {
        return Err(format!("规则包不存在：{id}@{version}"));
    }
    std::fs::remove_dir_all(&target).map_err(|error| error.to_string())
}

pub fn rule_origins(data_root: &Path) -> Result<std::collections::HashMap<String, String>, String> {
    let mut origins = std::collections::HashMap::new();
    for info in list(data_root) {
        if info.error.is_some() {
            continue;
        }
        let dir = PathBuf::from(&info.path);
        let manifest = read_manifest(&dir)?;
        let values = read_rule_values(&dir, &manifest)?;
        for value in values {
            if let Some(id) = value.get("id").and_then(|id| id.as_str()) {
                origins.insert(id.to_string(), format!("{}@{}", manifest.id, manifest.version));
            }
        }
    }
    Ok(origins)
}

/// 合并所有已安装包与用户自定义规则；无包时保留 custom 原文以保留原始错误。
pub fn combined_rules(data_root: &Path, custom: Option<&str>) -> Result<Option<String>, String> {
    let infos = list(data_root);
    if infos.is_empty() {
        return Ok(custom.map(|value| value.to_string()));
    }
    let mut rules = Vec::new();
    for info in infos {
        if let Some(error) = info.error {
            return Err(format!("已安装规则包 {}@{} 无效：{error}", info.id, info.version));
        }
        let dir = PathBuf::from(&info.path);
        let manifest = read_manifest(&dir)?;
        let mut values = read_rule_values(&dir, &manifest)?;
        rules.append(&mut values);
    }
    if let Some(custom) = custom {
        let value: serde_yaml::Value =
            serde_yaml::from_str(custom).map_err(|error| format!("自定义规则解析失败：{error}"))?;
        if let Some(items) = value.get("rules").and_then(|value| value.as_sequence()) {
            rules.extend(items.iter().cloned());
        }
    }
    if rules.is_empty() {
        return Ok(None);
    }
    Ok(Some(serialize_rules(rules)?))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_pack(root: &Path, id: &str, version: &str, rules: &str) {
        let dir = pack_dir(root, id, version);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("pack.yaml"),
            format!(
                "id: {id}\nversion: {version}\nengine_min: 1.0.0\nrules:\n  - rules.yaml\n"
            ),
        )
        .unwrap();
        std::fs::write(dir.join("rules.yaml"), rules).unwrap();
        let manifest = read_manifest(&dir).unwrap();
        let hash = pack_content_hash(&dir, &manifest).unwrap();
        std::fs::write(dir.join(".integrity"), hash).unwrap();
    }

    #[test]
    fn install_list_remove_and_combine() {
        let data = tempfile::tempdir().unwrap();
        let source = tempfile::tempdir().unwrap();
        std::fs::write(
            source.path().join("pack.yaml"),
            "id: com.example.rules\nversion: 1.0.0\nrules:\n  - rules.yaml\n",
        )
        .unwrap();
        std::fs::write(
            source.path().join("rules.yaml"),
            "rules:\n  - id: pack/one\n    language: typescript\n    dimension: security\n    severity: warning\n    message: m\n    pattern: |\n      (call_expression function: (identifier) @match) @match\n",
        )
        .unwrap();

        let installed = install(data.path(), source.path()).unwrap();
        assert_eq!(installed.id, "com.example.rules");
        assert_eq!(list(data.path()).len(), 1);

        let combined = combined_rules(data.path(), None).unwrap().unwrap();
        assert!(combined.contains("pack/one"));

        remove(data.path(), "com.example.rules", "1.0.0").unwrap();
        assert!(list(data.path()).is_empty());
        assert!(combined_rules(data.path(), None).unwrap().is_none());
    }

    #[test]
    fn rejects_unsafe_paths_and_duplicate_install() {
        let data = tempfile::tempdir().unwrap();
        let source = tempfile::tempdir().unwrap();
        std::fs::write(
            source.path().join("pack.yaml"),
            "id: safe\nversion: 1.0.0\nrules:\n  - ../escape.yaml\n",
        )
        .unwrap();
        assert!(install(data.path(), source.path()).is_err());

        write_pack(
            data.path(),
            "dup",
            "1.0.0",
            "rules:\n  - id: pack/dup\n    language: typescript\n    dimension: security\n    severity: warning\n    message: m\n    pattern: |\n      (call_expression function: (identifier) @match) @match\n",
        );
        let source2 = tempfile::tempdir().unwrap();
        std::fs::write(
            source2.path().join("pack.yaml"),
            "id: dup\nversion: 1.0.0\nrules:\n  - rules.yaml\n",
        )
        .unwrap();
        std::fs::write(
            source2.path().join("rules.yaml"),
            "rules:\n  - id: pack/dup\n    language: typescript\n    dimension: security\n    severity: warning\n    message: m\n    pattern: |\n      (call_expression function: (identifier) @match) @match\n",
        )
        .unwrap();
        assert!(install(data.path(), source2.path()).is_err());
    }

    fn write_source_pack(source: &Path, id: &str) {
        std::fs::write(
            source.join("pack.yaml"),
            format!("id: {id}\nversion: 1.0.0\nrules:\n  - rules.yaml\n"),
        )
        .unwrap();
        std::fs::write(
            source.join("rules.yaml"),
            format!(
                "rules:\n  - id: pack/{id}\n    language: typescript\n    dimension: security\n    severity: warning\n    message: m\n    pattern: |\n      (call_expression function: (identifier) @match) @match\n"
            ),
        )
        .unwrap();
    }

    #[test]
    fn signed_pack_requires_trusted_key_and_detects_tamper() {
        let data = tempfile::tempdir().unwrap();
        let source = tempfile::tempdir().unwrap();
        write_source_pack(source.path(), "signed");
        let (private, public) = generate_keypair().unwrap();
        sign_pack(source.path(), &private).unwrap();

        // 有签名但没有信任公钥 → 拒绝。
        assert!(install(data.path(), source.path()).is_err());

        let public = parse_public_key(&public).unwrap();
        let key = trust_key(data.path(), &public).unwrap();
        let info = install(data.path(), source.path()).unwrap();
        assert_eq!(info.signed_by.as_deref(), Some(key.key_id.as_str()));
        assert_eq!(list(data.path())[0].signed_by.as_deref(), Some(key.key_id.as_str()));

        // 篡改签名 → 校验失败，扫描拒绝。
        let dir = pack_dir(data.path(), "signed", "1.0.0");
        let signature_path = dir.join(SIGNATURE_FILE);
        let mut signature: PackSignature = serde_json::from_str(
            &std::fs::read_to_string(&signature_path).unwrap(),
        )
        .unwrap();
        signature.signature = base64::engine::general_purpose::STANDARD.encode([0u8; 64]);
        std::fs::write(
            &signature_path,
            serde_json::to_string_pretty(&signature).unwrap(),
        )
        .unwrap();
        assert!(list(data.path())[0].error.is_some());
        assert!(combined_rules(data.path(), None).is_err());

        untrust_key(data.path(), &key.key_id).unwrap();
        assert!(list(data.path())[0].error.is_some());
        assert!(trusted_keys(data.path()).unwrap().is_empty());
    }

    #[test]
    fn trust_key_rejects_unsigned_packs() {
        let data = tempfile::tempdir().unwrap();
        let source = tempfile::tempdir().unwrap();
        write_source_pack(source.path(), "unsigned");
        let (_, public) = generate_keypair().unwrap();
        let public = parse_public_key(&public).unwrap();
        trust_key(data.path(), &public).unwrap();
        let error = install(data.path(), source.path()).unwrap_err();
        assert!(error.contains("未签名"), "{error}");
    }

    #[test]
    fn malformed_trust_key_fails_closed() {
        let data = tempfile::tempdir().unwrap();
        let source = tempfile::tempdir().unwrap();
        write_source_pack(source.path(), "unsigned");
        install(data.path(), source.path()).unwrap();

        let trusted = trusted_keys_root(data.path());
        std::fs::create_dir_all(&trusted).unwrap();
        std::fs::write(trusted.join("bad.pub"), [1u8, 2, 3]).unwrap();
        assert!(trusted_keys(data.path()).is_err());
        assert!(list(data.path())[0].error.is_some());
        assert!(combined_rules(data.path(), None).is_err());
        let source2 = tempfile::tempdir().unwrap();
        write_source_pack(source2.path(), "unsigned-two");
        assert!(install(data.path(), source2.path()).is_err());
    }

    #[test]
    fn detects_tampered_pack_content() {
        let data = tempfile::tempdir().unwrap();
        let source = tempfile::tempdir().unwrap();
        std::fs::write(
            source.path().join("pack.yaml"),
            "id: com.example.tamper\nversion: 1.0.0\nrules:\n  - rules.yaml\n",
        )
        .unwrap();
        std::fs::write(
            source.path().join("rules.yaml"),
            "rules:\n  - id: pack/tamper\n    language: typescript\n    dimension: security\n    severity: warning\n    message: m\n    pattern: |\n      (call_expression function: (identifier) @match) @match\n",
        )
        .unwrap();
        install(data.path(), source.path()).unwrap();
        assert!(list(data.path())[0].error.is_none());

        let rule_file = pack_dir(data.path(), "com.example.tamper", "1.0.0").join("rules.yaml");
        std::fs::write(&rule_file, "rules: []\n").unwrap();
        assert!(list(data.path())[0].error.is_some());
        assert!(combined_rules(data.path(), None).is_err());
    }
}
