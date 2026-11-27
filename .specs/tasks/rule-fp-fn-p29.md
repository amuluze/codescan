---
status: done
---

# Task Spec：RulesView 结构拆分 P29

task-id: `rule-fp-fn-p29`

## 背景

1.6.0 扫描仍报告 `RulesView` 函数长度 374 行、圈复杂度 27。弹窗改造后规则页仍把目录表、规则包管理与页面状态混在一个函数中。

## 范围

- 抽出 `RuleTable`：规则表、启用开关、选择行。
- 抽出 `RulePacksSection`：规则包列表、安装入口、签名状态、卸载。
- `RulesView` 保留页面状态、加载、弹窗、目录选择器。
- 更新 DESIGN 组件清单与 CHANGELOG。

## 验收

- [x] 规则页表格/启停/选择/规则包安装卸载行为不变。
- [x] `RulesView` 不再触发 `metric/function-length`。
- [x] 前端规则相关测试通过。
- [x] 类型检查通过。
- [x] 全量门禁通过。

## 排除项

- 不改变规则页数据流或 Tauri IPC。
