---
status: done
---

# Task Spec：Provider Profile 弹窗与密钥统一配置

task-id: `provider-profile-modal`

## 目标

把新增/编辑 Provider Profile 从表格下方内联表单改为独立弹窗，并在同一个弹窗、同一次保存中提交 Profile 字段与 API Key：新建时可一并写入 Key；编辑时留空表示保持原 Key，可显式清除。移除独立的 Key 写入命令与页面内 Key 块。

## 现状（证据）

- 编辑表单内联在表格下方；新建必须先保存 Profile，出现 id 后才显示 Key 块，再单独「保存 Key」。
- Key 通过 `set_provider_api_key` / `clear_provider_api_key` 独立命令写入；Profile 通过 `upsert_provider_profile` 写入，二者无统一提交路径。
- 现有 1.5.5 已加入校验、端点预览、未保存确认与忙碌态，但这些逻辑分散在页面级状态里。

## 范围

1. **独立弹窗**
   - 新增 `ProviderEditorDialog`：名称、协议、Base URL、Model、API Key、清除 Key、端点预览、字段错误、未保存确认、保存/取消。
   - 弹窗打开时一次性载入 Profile 与 Key 状态；关闭时清空 Key 草稿；有修改时先确认放弃。
2. **统一保存**
   - 扩展 `upsert_provider_profile`：新增可选 `api_key` 与 `clear_key`。
   - 新建：Profile + 可选 Key 一次提交；编辑：Key 留空保持不变，填写则替换，勾选清除则删除。
   - 移除 `set_provider_api_key` / `clear_provider_api_key` 命令、前端 API、store action 与测试 mock。
   - 保存失败在弹窗内展示并保留草稿；成功后关闭弹窗并提示 Profile 名称。
3. **表格与校验**
   - 表格保留行高亮、忙碌态、测试/删除/设为默认提示；新建按钮始终可用。
   - 复用现有校验与端点预览逻辑，移入共享模块，弹窗与页面共用。
   - 窄屏卡片布局保持不变。
4. **测试与文档**
   - 后端无头测试：新建带 Key、编辑留空保持、显式清除、未知 id 拒绝。
   - 前端测试：弹窗打开/关闭、统一保存参数、Key 留空与清除、校验、未保存确认、失败保留。
   - 更新既有 Provider 测试、mock、docs、DESIGN、CHANGELOG。

## 验收标准

- [ ] 新增/编辑均以独立弹窗出现，不再在表格下方渲染内联表单。
- [ ] 新建弹窗可同时填写 Profile 与 API Key，一次调用 `upsert_provider_profile` 携带 `apiKey`，保存后关闭并显示已保存。
- [ ] 编辑已有 Key 的 Profile：Key 留空保存时 `apiKey=null` 且原 Key 不变；填写新 Key 时替换；勾选清除时 `clearKey=true`。
- [ ] 后端无头测试覆盖：新建带 Key 落库、留空保持、清除删除、未知 id 报错。
- [ ] 命令面与前端不再存在独立的 Key 写入/清除路径。
- [ ] 校验（名称、http(s) Base URL、Model）与端点预览/重复警告在弹窗内生效；保存无效时禁用。
- [ ] 有未保存修改时关闭弹窗先确认；失败时弹窗保持打开并显示可读错误。
- [ ] 现有 Provider 能力（测试连接、设为默认、删除、窄屏卡片）不回退；`task check` 全量通过。

## 排除项

- 不改 Provider 列表持久化结构、密钥存储位置或 Provider 协议。
- 不做跨标签页/刷新草稿保护。
- 不新增密钥查看明文能力。

## 关联

- Domain：`.specs/domain/scan-engine.md` §3a
- 现有实现：`SettingsView.tsx`、`llmStore.ts`、`commands.rs` Provider 命令、`App.test.tsx`
