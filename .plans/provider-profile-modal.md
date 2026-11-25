# Plan：provider-profile-modal

对应 Task Spec：`.specs/tasks/provider-profile-modal.md`。后端扩展一个命令参数，前端新增弹窗组件并删除分离的 Key 路径。

## 契约

```rust
upsert_provider_profile(id, name, kind, base_url, model, api_key: Option<String>, clear_key: Option<bool>) -> String
```

- `clear_key=true` → 删除该 Profile 的 secret。
- 否则 `api_key` 去空白后非空 → 写入 secret；`None`/空白 → 保持不变。
- Profile 校验在写任何数据之前完成；先写 Profile meta，再写 secret；secret 写入失败返回错误，弹窗保留草稿供重试。

## 共享模块

新增 `src/providerProfile.ts`：`ProviderEditorState`、`EMPTY_PROVIDER_EDITOR`、`isHttpUrl`、`profileFieldErrors`、`endpointPreview`、`endpointWarning`、`providerToEditor`。SettingsView 与 ProviderEditorDialog 共用，删除页面内重复实现。

## 步骤

1. 后端：把 `upsert_provider_profile` 主体抽成 `save_provider_profile(store, ...)`（无头可测），接收 key 参数；删除 `set_provider_api_key` / `clear_provider_api_key` 命令与注册。
2. 前端 API/store：`upsertProviderProfile(profile, {apiKey, clearKey})`；`saveProfile(profile, apiKey, clearKey)`；删除 `setProfileKey` / `clearProfileKey`。
3. 新组件 `ProviderEditorDialog.tsx`：本地草稿 + key 草稿 + 清除标记 + 校验/端点预览 + 未保存确认；提交 `{profile, apiKey, clearKey}`。
4. `SettingsView.tsx`：删除内联编辑器与相关状态；表格编辑/新建打开弹窗；保存成功关闭 + 卡片提示；错误传给弹窗。
5. 样式：弹窗宽度、字段布局、Key 行、清除勾选；移除内联编辑器专用样式；表格/窄屏保持不变。
6. 测试：后端无头（新建带 Key/留空/清除/未知 id）；前端 mock 与新用例；更新既有断言。
7. 文档：Domain §3a.2、`docs/llm.md`、`docs/en/llm.md`、DESIGN、CHANGELOG。
8. 验证：`pnpm typecheck && pnpm test && pnpm build`、`task check`、浏览器实拍弹窗与窄屏。

## 风险

- **半写入**：Profile 已保存而 secret 写入失败时返回错误；重试即可，Key 不会写到错误 Profile。
- **误清除**：清除必须显式勾选；留空默认不变。UI 勾选清除时禁用 Key 输入。
- **兼容**：删除独立 Key 命令后，确认无其他调用方（当前仅 SettingsView 与测试）。
