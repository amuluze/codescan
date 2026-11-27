---
status: done
---

# Task Spec：规则包 CLI

task-id: `rule-pack-cli`

## 范围

- 将规则包 manifest、安装/列出/卸载、内容哈希、规则合并逻辑从桌面命令层下沉到 `codescan-engine`，桌面与 CLI 共用一份实现。
- CLI 新增 `pack install <source>`、`pack list [--json]`、`pack remove <id> <version> [--json]`。
- 数据根：`CODESCAN_DATA_DIR`，未设置时回退 `~/.codescan`。
- `codescan scan` 自动合并已安装规则包与 `--rules`；包无效或被篡改时拒绝扫描（退出码 2）。
- 安装失败不留下半安装目录；重复安装拒绝；路径穿越/非法 manifest/引擎不兼容/规则冲突拒绝。

## 验收

- [x] 桌面端改用 `codescan_engine::packs`，行为不变。
- [x] `pack install` 安装合法目录并输出 id/version/规则数。
- [x] `pack list --json` 输出 id/version/integrity/error，可脚本消费。
- [x] `pack remove` 只删除指定版本。
- [x] 已安装包规则参与 CLI 扫描；卸载后不再命中。
- [x] 非法包安装退出码 2。
- [x] `task check` 全绿。

## 排除项

- 包签名、信任模型见 `rule-pack-signing-v1.md`；远程下载/更新与注册表不在本任务。
- CLI `scan` 不读取桌面自定义规则；扫描仅合并已安装包与显式 `--rules`。安装校验沿用共享实现，会检查同一数据根下 `custom.yaml` 的 id 冲突。
