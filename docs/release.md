# 发布链与签名公证（macOS Apple Silicon）

发布认证链参照 axiom 仓库的实证链移植：凭据缺失时**自动降级**（adhoc 未签名 +
updater 配置剥离），构建不失败；凭据齐全时 Developer ID 签名 + hardened runtime
+ notarytool 公证 + staple + minisign 更新工件。

## 链路

```
bump 版本（package.json / apps/desktop/package.json / tauri.conf.json 三处）
  → 打 tag（v*）推 origin
  → .github/workflows/release.yml（macos-15，arm64 单目标）：
      Secrets 条件映射（非空才注入 GITHUB_ENV）
      → scripts/ci-macos-credentials.sh（.p12 → 临时钥匙串 / .p8 → 落盘）
      → pnpm run build:dmg（generate-release-config + tauri sign-only 构建）
      → xcrun notarytool 公证 DMG（3 次重试）+ stapler staple/validate
      → scripts/release-signing-check.sh 签名验收
      → scripts/generate-latest-json.mjs + gh release create --draft + 资产上传
  → 手动 publish draft release（updater 端点在 publish 后生效）
```

本地发布：`task build:dmg`（自动 source `~/.codescan/release-credentials.env`），
产物在 workspace 根 `target/aarch64-apple-darwin/release/bundle/`（Cargo workspace 约定，不在 src-tauri 下）。

## 凭据约定

本地凭据文件 `~/.codescan/release-credentials.env`（0600，不入库，可选——缺失即降级）：

| 用途 | 变量 | 说明 |
| --- | --- | --- |
| 签名身份 | `APPLE_SIGNING_IDENTITY` | 值须与 `security find-identity -v -p codesigning` 输出一致；CI 由 .p12 自动发现 |
| 签名证书（CI） | `APPLE_CERTIFICATE` + `APPLE_CERTIFICATE_PASSWORD` | .p12 的 base64；仅经 step 级 env 给 `ci-macos-credentials.sh`，不进构建环境（tauri 对该变量只判「存在与否」，转发会二次导入冲突） |
| 钥匙串密码（CI） | `KEYCHAIN_PASSWORD` | 缺省 `codescan-ci` |
| 公证（方案 A，推荐） | `APPLE_API_ISSUER` + `APPLE_API_KEY`（Key ID 或 .p8 路径/内容）+ `APPLE_API_KEY_PATH` | App Store Connect API Key；本地模式 `ci-macos-credentials.sh` 把内容/路径归一成 `APPLE_API_KEY_PATH` |
| 公证（方案 B，备选） | `APPLE_ID` + `APPLE_PASSWORD`（app-specific）+ `APPLE_TEAM_ID` | |
| updater 签名 | `TAURI_SIGNING_PRIVATE_KEY`（base64 包裹单行）+ `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | 密钥对 `~/.tauri/codescan-v1.key`（key ID A74549A0F0FCE20C）；**更换密钥必须同步更新 `tauri.release.conf.json` 的 pubkey，否则老客户端无法校验更新** |

CI Secrets（amuluze/codescan）：`APPLE_CERTIFICATE`、`APPLE_CERTIFICATE_PASSWORD`、
`APPLE_API_ISSUER`、`APPLE_API_KEY_ID`、`APPLE_API_KEY_P8`、
`TAURI_SIGNING_PRIVATE_KEY`、`TAURI_SIGNING_PRIVATE_KEY_PASSWORD`。
本地与 CI 同一套 Apple 开发者凭据（Developer ID Application，.p12 备份与
App Store Connect API Key 同 axiom 机器约定存放）。

## updater（自动更新）

- 端点：`https://github.com/amuluze/codescan/releases/latest/download/latest.json`
  （静态直链；**draft release 发布后生效**。当前仓库为 private——未公开时该端点与
  release 页面对未认证访问一律 404，应用内检查更新会失败；仓库公开后**无需任何
  改动**，端点自动生效）。
- 工件：`Codescan.app.tar.gz` + `.sig`（`createUpdaterArtifacts`，bundler 产出），
  CI 重命名为 `Codescan_<v>_aarch64.app.tar.gz(.sig)`；`latest.json` 由
  `scripts/generate-latest-json.mjs` 生成（signature 取 .sig 全文）。
- 插件常驻编译；仅当配置存在 `plugins.updater`（`generate-release-config.mjs`
  按凭据注入）时 `lib.rs` 才注册 updater 插件——开发/无密钥构建保持未配置。
- 签名验收：`tauri signer verify -k ~/.tauri/codescan-v1.key <tar.gz> <tar.gz.sig>`。

## 验收命令

```bash
# 签名验收（正式链 fail-closed；本地调试加 --allow-adhoc）
task release:signing-check -- target/aarch64-apple-darwin/release/bundle/macos/Codescan.app
# updater 工件验签（同 tauri-plugin-updater 更新时实现；须先 task build:dmg）
cargo test -p codescan-desktop --test updater_signature -- --ignored --nocapture
# 降级路径回归（无凭据构建应成功且产物 adhoc、无 updater 工件）
HOME=$(mktemp -d) task build:dmg
```

## 故障排查

- **DMG 签名报 `A timestamp was expected but was not found`**：RFC 3161 时间戳
  服务 `timestamp.apple.com` 不可达（axiom 实跑踩坑），需系统代理；探针
  `codesign --force --sign "$APPLE_SIGNING_IDENTITY" --timestamp <任意已签对象>`。
- **CI 无头环境 codesign 挂死**：临时钥匙串缺 `set-key-partition-list`
  （`ci-macos-credentials.sh` 已内置，勿删）。
- **公证 Accepted 但 staple 失败**：回执拉取是独立网络交互（NSURLError -1005
  同型），重试即可；workflow 对 notarytool 与 stapler 各做 3 次重试。
- **GHA 未配置 secret 注入空字符串**：条件映射必须判「非空」而非「已定义」，
  否则 updater/签名带空凭据激活、构建在最后签名一步失败。
