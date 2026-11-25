---
status: done
---

# Task Spec：Provider Profile 增改操作逻辑

task-id: `provider-profile-editor`

## 目标

让设置页「模型 Provider」卡片中的新增/编辑流程可预测、可校验、无密钥串档：编辑目标明确、未保存修改有保护、表单提交前校验、端点可预览、异步操作有忙碌态。

## 现状问题（证据）

- `apiKey` 状态在切换 Profile 或关闭编辑器时不清空；先给 A 输入 Key、再打开 B，会把 A 的 Key 存到 B。
- `取消`/切换 Profile/新建会静默丢弃未保存修改；编辑中的行没有高亮或标题。
- 保存前无客户端校验；空 Base URL/Model 也会发请求，错误只在页面顶部展示（现有测试甚至断言发送空 `baseUrl`）。
- 保存已有 Profile 成功后编辑器仍保持打开，`取消` 语义变成“关闭但已保存”；新建保存后需要保留编辑器配置 Key。
- Base URL 的实际请求路径不透明：OpenAI 追加 `/chat/completions`，Anthropic 追加 `/v1/messages`，带 `/v1` 的 Anthropic Base 会重复。
- 测试结果不带 Profile 名称；设为默认/删除无忙碌态，可重复触发。

## 范围

1. **编辑状态机**
   - 编辑器标题区分「新建 Provider Profile」/「编辑 Provider Profile：名称」；正在编辑的行高亮。
   - 打开其他 Profile、新建、关闭时若存在未保存修改（字段或 Key 草稿），先展示内联确认「放弃修改 / 继续编辑」。
   - 每次打开/关闭编辑器清空 Key 草稿与编辑器错误。
2. **客户端校验**
   - 名称、Base URL、Model 必填；Base URL 必须是 http(s)。
   - 保存按钮在无效时禁用并给出提示；Base URL 非空且非法时显示字段级错误。
   - Base URL 字段占满整行；显示「实际请求」预览与重复路径警告。
3. **保存语义**
   - 编辑已有 Profile 保存成功后关闭编辑器并在卡片内提示。
   - 新建保存成功后保留编辑器以配置 Key，按钮显示「关闭」，并提示已保存。
4. **密钥与异步操作**
   - Key 草稿永不跨 Profile 复用；Key 块标题带 Profile 名称。
   - 保存 Profile / 保存 Key / 清除 Key / 设为默认 / 删除均有忙碌态，避免重复请求。
   - 测试结果标注被测试的 Profile 名称；删除/切换默认成功后在卡片内提示。
5. **表格**
   - Base URL/Model 过长时省略并提供完整值 tooltip；窄屏不撑破卡片。
6. **测试与文档**
   - 更新既有 Provider 设置测试；新增切换清空 Key、未保存保护、校验/端点预览、测试结果命名用例。
   - 同步 `docs/llm.md`、`docs/en/llm.md`、CHANGELOG。

## 验收标准

- [ ] 输入 Key 后切换到另一 Profile 会先提示未保存；放弃后新编辑器 Key 输入为空。
- [ ] 修改字段后点击「关闭」或切换 Profile 会出现内联确认；「继续编辑」保留草稿，「放弃修改」执行切换/关闭。
- [ ] 新建时名称/Model/Base URL 任一为空或 Base URL 非 http(s) 时保存禁用并提示；补全后可保存。
- [ ] Base URL 预览按协议显示实际请求 URL；Anthropic Base 以 `/v1` 结尾时显示重复追加警告。
- [ ] 编辑已有 Profile 保存成功后编辑器关闭；新建保存成功后编辑器保留并出现 Key 块与「关闭」。
- [ ] 保存/切换默认/删除进行中按钮禁用且不会重复 invoke。
- [ ] 测试结果包含 Profile 名称；删除与设为默认成功后有卡片内提示。
- [ ] 现有 32 项前端测试更新后全部通过，`task check` 全量通过。

## 排除项

- 不改 Provider 持久化结构、密钥存储或后端校验规则。
- 不做系统原生确认框；不做跨标签页/刷新前的草稿保护。
- 不新增「清除 Key」二次确认（可重新输入恢复）。

## 关联

- Domain：`.specs/domain/scan-engine.md` §3a
- 现有实现：`SettingsView.tsx`、`llmStore.ts`、`commands.rs` Provider 命令
