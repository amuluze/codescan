---
status: done
---

# Task Spec：桌面端规则编写 MVP

task-id: `rule-authoring-ux-v1`

## 背景

路线图第 4 项「桌面端规则编写体验」要求把当前只能手写 `~/.codescan/rules/custom.yaml` 的流程收进应用。v1 只覆盖自定义规则，不引入规则包编辑器。

## 范围

- 规则页选择规则后展示详情：语言、维度、严重级、说明、规则体；内置规则只读。
- 自定义规则可打开完整 YAML 编辑器：
  - 读取当前 `custom.yaml`；
  - 校验（按当前启停状态编译）；
  - 原子保存（临时文件 + rename，失败不覆盖原文件）；
  - 未保存切换确认。
- 测试工作台：
  - 选择语言、编辑样例代码；
  - 仅启用当前自定义规则文件，对临时工作区运行扫描；
  - 展示实际命中行、规则 id、消息与命中文本。
- 规则错误行仍走既有 `RuleInfo.error` 展示。

## 验收标准

- [x] 自定义规则行可打开编辑器，textarea 加载完整 custom.yaml。
- [x] 校验调用 `validate_custom_rules`，错误逐条展示。
- [x] 保存调用 `save_custom_rules`，失败不写入且显示错误。
- [x] 测试调用 `test_custom_rule`，仅候选规则参与扫描，命中行可见。
- [x] 内置规则只读，展示 source。
- [x] 前端 Vitest 覆盖编辑/校验/保存/测试主链路。
- [x] Headless 测试覆盖 `test_custom_rule` 仅启用候选规则与非法语言错误。
- [x] `task check` 全绿。

## 排除项

- 规则包编辑/安装；多文件规则项目；tree-sitter 节点自动补全；规则市场。
- 内置规则编辑；规则 id 冲突自动修复。
- 测试工作台的 fixture 标注语义（当前为实际命中烟测，不是完整 A2 fixture 回归）。

## 关联

- Domain：§4.4 自定义规则与内置规则冲突处理；§3.3 编译校验范围。
- 实现：`commands.rs` get/validate/save/test、`RulesView.tsx`、`platform.ts`、`rulesStore.ts`。
- 后续：规则包 manifest 与安装（路线图 3），以及完整 fixture 回归编辑器。
