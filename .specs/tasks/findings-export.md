# Task Spec: 发现页导出（展示层）

## 目标

在发现页导出当前筛选与排序后的确定性发现，便于离线审阅与对接外部工具。导出只读展示数据：不写数据库、不改基线、不参与门禁。

## 范围

1. 入口：发现页工具栏「导出」按钮；无扫描上下文（`lastScanId` 为空）或当前视图为空时禁用；点击后经 `DirectoryPicker` 选择目录。
2. 内容：当前 `filteredFindings` 结果（与页面展示一致），输出 Markdown 与 JSON 两个文件，文件名 `codescan-findings-<scanId>.md|json`。
3. Markdown 字段：严重级、维度、位置（`path:line:column`）、规则 id、说明、命中文本、污点链、指纹；头部声明确定性来源，不记录导出时间（Domain §2）。
4. 落盘：复用命令 `export_report(dest_dir, base, markdown, json)`；`base` 仅允许 ASCII 字母数字与 `-`、`_`（防路径穿越）；错误可读且不改变页面状态。
5. 失败语义：目录不存在、无写权限、文件名非法 → 可读错误；不写入 findings / baseline。

## 验收标准

- [x] 前端测试：筛选后导出调用 `export_report`，`base` 为 `codescan-findings-<scanId>`，内容只含筛选结果。
- [x] 后端测试：写入 Markdown 与 JSON 两个文件；空目录、不存在目录、非法 `base` 返回可读错误。
- [x] DESIGN.md 发现页结构含导出入口。

## 排除项

- 不替代 SARIF 导出：历史页仍保留整次扫描的 SARIF 导出。
- 不新增持久化与历史；不做远端上传。

## 关联

- 复用：`.specs/tasks/llm-code-review.md` 引入的 `export_report` 与目录选择流程。
- Domain：`.specs/domain/scan-engine.md` §2（确定性）——导出不含时间等机器差异。
