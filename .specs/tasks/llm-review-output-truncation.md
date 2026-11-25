---
status: done
---

# Task Spec：LLM 审查正文被输出预算截断时按截断处理并降批重试

task-id: `llm-review-output-truncation`

## 目标

审查响应正文（JSON 数组）被 Provider 输出预算截断（`max_tokens`）时，识别为可重试的截断错误，走既有「二分降批 / 按行切分」恢复路径，而不是落到 `响应不是 JSON 数组` 的不可恢复错误并中止整页。

## 现象与复现（证据）

- 报错：`响应不是 JSON 数组：[ { "path": "codescan/crates/engine/src/taint/engine.rs", "line": 103, ...`，正文在数组中间被截断（首字符为 `[`，全响应无 `]`）。
- 调用链：`review()` → `parse_anthropic_response` / `parse_openai_response` → `parse_review` → `json_array_slice`（`llm.rs:1076`）要求存在 `]`，缺失即返回错误。
- 现有实现只在 Anthropic **无正文**（仅 thinking 块）时检查 `stop_reason == max_tokens`（`llm.rs:231`）；有正文时忽略 `stop_reason`，OpenAI 响应则完全不读 `finish_reason`。因此正文截断不会被 `is_truncation_error`（`llm.rs:1535`）识别，`review_batched` 的降批分支不触发。
- Domain §3b.4 只写了「思考预算截断」，未覆盖正文输出预算截断，属于契约缺口。

## 范围

1. 解析层：OpenAI `choices[0].finish_reason == "length"` 与 Anthropic `stop_reason == "max_tokens"` 且存在正文时，返回可读截断错误（不含原始 JSON），不返回残缺正文。
2. 重试判定：`is_truncation_error` 同时识别 thinking 阶段与正文阶段截断。
3. `review_batched` 沿用既有二分/按行切分路径；不改批次上限、游标、部分结果与复核语义。
4. 契约与文档：Domain §3b.4 改为「输出预算截断（thinking 或正文）」；`docs/llm.md`、README 相关说明与 `CHANGELOG.md` 同步。

## 验收标准

- [ ] 单测：OpenAI 正文残缺 + `finish_reason: length` 返回错误且 `is_truncation_error` 为真；Anthropic 正文残缺 + `stop_reason: max_tokens` 同理；未截断的正常响应仍解析成功。
- [ ] 集成测试：首批返回正文截断的 Anthropic 响应，`review_batched` 二分后继续请求并汇总两批建议，`error` 为空。
- [ ] 用户不再看到以截断 JSON 明文结尾的 `响应不是 JSON 数组`；错误包含截断原因与可操作建议。
- [ ] `cargo test -p codescan-desktop --lib llm::` 与 `task check` 通过。

## 排除项

- 不做残缺 JSON 的容错解析（不猜测未闭合数组）。
- 不改变 `REVIEW_MAX_TOKENS`、批次大小或 Provider 重试策略。
- 不处理流式解析路径的截断（流式仅用于人工查看分诊）。

## 关联

- Domain：`.specs/domain/scan-engine.md` §3b.4
- 实现：`codescan/apps/desktop/src-tauri/src/llm.rs`
- 前序任务：`.specs/tasks/llm-review-retry-partial.md`
