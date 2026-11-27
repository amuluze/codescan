---
status: done
---

# Task Spec：自定义规则编辑弹窗

task-id: `rule-authoring-modal-v1`

## 范围

- 新建自定义规则从页面内嵌编辑器改为 `RuleEditorDialog` 弹窗。
- 新建弹窗保存时调用 `save_custom_rule`，只追加单条规则，不覆盖既有 `custom.yaml`。
- 编辑既有自定义规则仍打开同一弹窗，保存完整 `custom.yaml`。
- 保留 YAML 校验、测试样例、语言选择、错误列表与未保存关闭确认。
- 更新 DESIGN 组件/弹层清单与前端测试。

## 验收

- [x] 点击「新建自定义规则」打开 `role="dialog"` 弹窗。
- [x] 新建保存调用 `save_custom_rule`，不调用 `save_custom_rules`；成功后关闭弹窗。
- [x] 编辑既有规则仍可校验、保存完整 `custom.yaml` 并运行测试。
- [x] 前端测试、类型检查与全量门禁通过。

## 排除项

- 可视化规则表单（当前仍以 YAML 为唯一输入）。
