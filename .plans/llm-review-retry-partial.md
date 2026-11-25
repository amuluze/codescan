# Plan：llm-review-retry-partial

对应 Task Spec：`.specs/tasks/llm-review-retry-partial.md`。

## 重试

- 抽出 `send_completion_request(client, config, key, body, retries)`：每次重建 Request，读取状态与 `Retry-After`，2xx 返回正文，可重试状态且仍有次数则 sleep 后继续，否则返回最终错误。
- `review()` 与复核（`complete_with_timeout(..., retries=5)`）复用；分诊保持 0 次重试。
- `RETRY_BASE_DELAY_MS` 在 `cfg(test)` 下为 0；纯函数 `retry_delay` 接收 base，便于单测。

## 部分结果

```rust
pub struct ReviewBatchOutcome {
    suggestions, dropped, rejected,
    reviewed_files, reviewed_bytes,
    error: Option<String>,
}
```

- 每成功一批累加文件数/字节数；不可恢复错误记录 `error` 并停止。
- 截断二分仍继续；复核失败仍降级。
- 命令：`next_offset = error ? page_offset + reviewed_files : selection.next_offset`，并把 `error` 放进 `ReviewResult`。
- 前端累加已完成计数（游标已按完成文件推进，不会重复），有 `error` 时停自动续审。

## 步骤

1. `llm.rs`：重试 helper、`ReviewBatchOutcome`、`review_batched`/`review`/`verify_review` 改造。
2. `commands.rs`：结果映射与游标 helper。
3. `platform.ts`/`llmStore.ts`：`error` 字段、部分结果累积与停止条件。
4. 测试：纯函数、529 重试成功/耗尽、两批部分失败、命令游标、前端部分结果。
5. 文档与门禁：Domain v9、docs、CHANGELOG，`task check`，必要时实拍错误态。
