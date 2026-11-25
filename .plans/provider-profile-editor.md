# Plan：provider-profile-editor

对应 Task Spec：`.specs/tasks/provider-profile-editor.md`。仅前端操作逻辑与样式，后端命令与存储不变。

## 状态机

```text
editor: {id,name,kind,baseUrl,model} | null
baseline: 与 editor 同形（打开时的持久化值）
apiKey: string（仅属于当前 editor）
dirty = editor != baseline || apiKey != ""
pending: {kind:"open", target} | {kind:"close"} | null
```

- `requestOpen(target)` / `requestClose()`：dirty 时记 pending 并显示内联确认，否则立即执行。
- `confirmDiscard()`：执行 pending；`继续编辑` 清空 pending。
- 所有 open/close 路径清空 `apiKey`、`profileError`、`savedNotice`。

## 校验与预览

- `profileFieldErrors(editor)`：名称/Model 非空、Base URL 非空且可被 `new URL` 解析为 http(s)。
- 保存按钮 `disabled = saving || pending || invalid`；footer 提示缺失字段。
- `endpointPreview(editor)`：OpenAI `{base}/chat/completions`，Anthropic `{base}/v1/messages`。
- 警告：Anthropic Base 以 `/v1` 或 `/v1/messages` 结尾、OpenAI Base 以 `/chat/completions` 结尾。

## 保存

- 已有 Profile：成功 → 关闭编辑器 + 卡片提示。
- 新建：成功 → 保留编辑器（id 回填、baseline 更新）+ Key 块 + 「关闭」。
- 失败 → 编辑器内 `role="alert"`，保留草稿。

## 步骤

1. `SettingsView.tsx`：新增 baseline/pending/busy/error 状态与 helpers；替换 save/saveKey/removeProfile。
2. 表格：编辑行高亮、URL 省略 tooltip、按钮忙碌态、测试结果带名称。
3. 编辑器：标题、内联错误、未保存确认、端点预览、Key 块命名、动态关闭按钮。
4. `global.css`：行高亮、错误/警告、编辑器头、端点预览、URL 省略。
5. 更新/新增 `App.test.tsx` 用例；文档与 CHANGELOG。
6. 验证：`pnpm typecheck && pnpm test && pnpm build`，浏览器实拍 1280/640 的编辑、确认与校验态。
