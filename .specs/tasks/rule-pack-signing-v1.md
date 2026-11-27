---
status: done
---

# Task Spec：规则包签名与信任

task-id: `rule-pack-signing-v1`

## 范围

- 签名格式：分离式 `.signature` JSON，算法固定 `ed25519`，签名字节 = `codescan-pack-integrity:v1:<内容哈希>`。
- 信任库：`<data_root>/trusted-keys/<sha256(公钥)>.pub`，文件内容为 32 字节 Ed25519 公钥。
- 策略：
  - 未配置信任公钥：允许未签名包；拒绝已签名包（无法验证）。
  - 已配置信任公钥：所有包必须由受信任公钥签名；未签名包拒绝。
  - 任何签名校验失败、签名格式错误、密钥未受信任均拒绝安装与扫描。
- CLI：
  - `pack keygen --out <dir>` 生成 `private.key` / `public.key`；私钥在 Unix 下权限 0600，已存在则拒绝覆盖。
  - `pack sign <source> --key <private.key>` 计算包内容哈希并写入 `.signature`。
  - `pack trust add|list|remove` 管理信任公钥。
- 安装复制 `.signature`；列表返回 `signed_by`；扫描沿用列表校验，篡改/未信任签名即失败。
- 桌面端复用同一实现，规则包表显示已签名状态。

## 验收

- [x] 签名包在无信任公钥时拒绝安装。
- [x] 信任公钥后可安装、扫描，列表返回 `signed_by`。
- [x] 篡改签名后列表报错，扫描/规则合并拒绝。
- [x] 配置信任公钥后未签名包拒绝安装。
- [x] CLI keygen/sign/trust 全流程测试通过。
- [x] 私钥不进入日志、扫描历史或列表输出。
- [x] `task check` 全绿。

## 排除项

- 密钥撤销/轮换、多个并行签名、远程注册表、信任公钥 UI。
- 包签名不替代代码审查：受信任密钥签名的包仍按普通规则校验编译与 id 冲突。
