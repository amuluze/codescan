---
status: done
---

# Task Spec：本地规则包 MVP

task-id: `rule-pack-v1`

## 范围

- 包目录：`~/.codescan/packs/<id>/<version>/`，必需 `pack.yaml`。
- manifest：id、version、可选 engine_min/engine_max、description、rules 文件列表。
- 桌面命令：list/install/remove；安装校验 manifest、路径安全、引擎版本、规则编译与内置/既有包/自定义规则 id 冲突。
- 扫描加载：内置 + 已安装包 + 自定义规则合并；包规则只读，来源显示 `<id>@<version>`。
- 扫描历史 ruleset 快照记录包 id/version/integrity；历史页可查看。
- 安装写入内容哈希 `.integrity`，加载/列表校验；篡改导致包无效。
- 失败不留下半安装目录；卸载只删除指定包版本。

## 验收

- [x] 安装/列出/卸载本地包。
- [x] 非法 manifest、路径穿越、引擎不兼容、规则编译/冲突被拒绝。
- [x] 安装失败清理临时目录；重复安装拒绝。
- [x] 扫描使用包规则；规则页显示包来源。
- [x] 包规则不可在自定义编辑器中修改。
- [x] 扫描历史记录并展示包 id/version/integrity。
- [x] 内容哈希篡改检测与测试。
- [x] `task check` 全绿。

## 排除项

- 签名、远程下载/更新、包注册表（CLI `pack` 命令见 `rule-pack-cli.md`）。
- 包规则覆盖同名内置规则；同名 id 仍整体拒绝。
