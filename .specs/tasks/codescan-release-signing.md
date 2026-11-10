---
status: done
---

# Task Spec：发布认证（签名 + 公证 + 自动更新）

task-id: `codescan-release-signing`

## 背景与目标

`codescan-ops-v4` 遗留缺口：打包产物未签名（macOS 首开需右键 → 打开）、无自动更新。
参照 axiom 仓库的实证发布认证链移植（该链把实战踩坑固化在脚本注释：keychain 分区表
防挂死、空串 secret 误判、GHA 上 tauri 原生公证挂死、staple 回执独立网络交互、
macos-14 镜像下线）。设计原则与 ops-v4 一致：**凭据缺失 = 自动降级（adhoc 未签名 +
updater 配置剥离），构建不失败**。

## 范围与验收

### 1. 签名 + 公证

- `tauri.release.conf.json`（入库）：`hardenedRuntime` + `entitlements.plist`
  （allow-jit / allow-unsigned-executable-memory / disable-library-validation）；
  adhoc 构建时 entitlements 休眠。
- `scripts/ci-macos-credentials.sh`：CI 模式 .p12 → 临时钥匙串（分区表 + 防锁定）
  → 自动发现 Developer ID 身份写 GITHUB_ENV；.p8 落盘 → `APPLE_API_KEY_PATH`；
  本地模式归一 `APPLE_API_KEY` → PATH。原始密钥材料不进构建环境。
- CI（release.yml，macos-15）：sign-only 构建（公证显式做——tauri 原生公证在 GHA
  挂死）→ notarytool 公证 DMG（3 次重试）→ stapler staple/validate（3 次重试）。
- 验收：有凭据本地 `task build:dmg` 后 `codesign --verify --deep --strict` /
  `spctl --assess --type execute` / `stapler validate` 全过（`release-signing-check.sh`
  fail-closed；`--allow-adhoc` 仅放行调试产物）。

### 2. 自动更新（updater）

- `tauri-plugin-updater` + `tauri-plugin-process`（relaunch）常驻编译；仅当配置存在
  `plugins.updater` 时 lib.rs 才注册（Config 反序列化不接受 null，无配置注册会阻断启动）。
- `scripts/generate-release-config.mjs`：updater 启用判定 = 提交 pubkey 非空 且
  `TAURI_SIGNING_PRIVATE_KEY` 非空（判「非空」非「已定义」，规避 CI 空 secret）；
  禁用时剥离 `createUpdaterArtifacts` + `plugins.updater` → 产出 gitignore 的
  `tauri.ci.conf.json`。
- minisign 密钥对 `~/.tauri/codescan-v1.key`（key ID A74549A0F0FCE20C），pubkey 提交于
  `tauri.release.conf.json`；端点 `https://github.com/amuluze/codescan/releases/latest/download/latest.json`
  （draft 发布后生效）；`scripts/generate-latest-json.mjs` 产出 v2 清单（signature 取
  .sig 全文，平台键 `darwin-aarch64`）。
- 设置页「应用更新」卡片：检查 → 展示版本/说明 → 下载安装并重启；失败优雅报错
  （开发模式无已发布 release 属预期）。IPC 边界收口在 `platform.ts`（`updater.check`
  / `installAndRelaunch`）。
- 验收：无 key 生成配置被剥离（`generate:release-config` 输出 updater=disabled 且
  配置无 updater 字段）；有 key 产出 `.app.tar.gz` + `.sig` 且 `tauri signer verify`
  通过；vitest 覆盖检查更新成功/失败两分支。

### 3. 凭据注入（一次性运维）

- 本地 `~/.codescan/release-credentials.env`（0600 不入库）：复用 axiom 同一套 Apple
  开发者凭据 + codescan 专属 updater 密钥。
- `gh secret set` 注入 amuluze/codescan：`APPLE_CERTIFICATE`（p12 base64）、
  `APPLE_CERTIFICATE_PASSWORD`、`APPLE_API_ISSUER`、`APPLE_API_KEY_ID`、
  `APPLE_API_KEY_P8`、`TAURI_SIGNING_PRIVATE_KEY(+PASSWORD)`。
- 验收：`gh secret list` 七项齐备；任何凭据文件不入库（gitignore 兜底
  `tauri.ci.conf.json`，凭据仅在 HOME）。

## 排除项

macOS Intel（x86_64）构建、Homebrew/AUR 分发、Windows Authenticode、latest.json 托管
站点（用 GitHub Releases 直链）、版本号 bump 自动化。

## 失败语义

- 无凭据构建：降级 adhoc，不视为失败（与 ops-v4 一致）。
- 有签名身份但公证/staple 失败：CI fail-closed（重试 3 次后判红），产物不得出门。
- updater 检查失败：UI 错误态，不阻断页面。

## 结论（2026-11-10，已实施）

1. **签名 + 公证**：`entitlements.plist` + `tauri.release.conf.json`（hardenedRuntime，
   entitlements 三项 JIT/WebKit/库校验注释）；`scripts/ci-macos-credentials.sh`
   （p12 → 临时钥匙串 + 分区表/防锁 + 身份发现，p8 落盘，本地模式归一 API key 路径）；
   `scripts/release-signing-check.sh`（Developer ID 身份 → codesign verify --deep
   --strict → spctl → staple 校验「app/dmg 取其一」，本地链 stapled app / CI 链
   stapled dmg，adhoc 需 `--allow-adhoc` 显式放行）。实测：`task build:dmg` 全凭据链
   Developer ID 签名 → bundler 公证 Accepted → staple/validate 全过（验收 exit 0）。
   注意：产物路径在 **workspace 根 `target/`**（Cargo workspace 约定），不在
   src-tauri 下（首跑踩坑已修正 Taskfile/workflow/docs）。
2. **自动更新**：`tauri-plugin-updater` + `tauri-plugin-process` 常驻编译、按配置
   存在注册（lib.rs）；`generate-release-config.mjs` 按「非空」双条件判定并剥离；
   minisign keypair `~/.tauri/codescan-v1.key`（A74549A0F0FCE20C，pubkey 已提交）；
   端点 GitHub Releases 直链（draft 发布后生效）；`generate-latest-json.mjs` v2 清单
   （实测输出 darwin-aarch64 条目）；设置页「应用更新」卡片 + platform.ts 收口；
   vitest 15 例全绿（新增检查更新成功/失败分支）；发布验收测试
   `cargo test -p codescan-desktop --test updater_signature -- --ignored`（minisign-verify
   与 updater 客户端同实现）对真实工件验签通过。tauri npm 侧 api/cli 升 2.12.4
   对齐 crate 2.12.5（加插件后 minor 漂移触发版本一致性检查报错）。
3. **凭据**：`~/.codescan/release-credentials.env`（0600，复用 axiom Apple 凭据 +
   codescan 专属 updater key）；`gh secret set` 注入 amuluze/codescan 七项
   （`gh secret list` 已核实）。降级实测：`HOME=$(mktemp -d) task build:dmg` →
   构建成功、`updater=disabled`、产物 `Signature=adhoc`（TeamIdentifier not set）。

遗留：CI 全链实跑需推 tag 验证；latest.json 端点在首个 release publish 后生效。
