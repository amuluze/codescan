---
status: done
---

# Task Spec：Codescan v3.2 —— LLM 层增强（流式 / 批量分诊 / taint 候选）

task-id: `codescan-llm-v32`

## 背景

v3.1 的 LLM 层为一次性非流式调用、单发现分诊、候选规则仅 pattern/text。本次增强：流式输出、批量分诊、候选规则允许 taint 块。纪律不变（Domain §3a/§5.3）：所有 LLM 产出仅展示参考，不写发现/基线/门禁；只有校验并显式保存的规则影响后续扫描。

## 范围与验收

### 1. 流式调用

- Rust：SSE 解析纯函数（openai `data: {choices[0].delta.content}`；anthropic `content_block_delta.delta.text`），命令侧增量 emit `llm://delta` 事件（载荷含 requestId 与文本片），结束返回完整文本。
- 验收 E1：SSE 解析单测（两种 provider、分片跨行、`[DONE]`/空 delta、错误 JSON 容错）；UI 订阅 delta 并增量渲染；无 Key/网络错误时命令返回可读错误。

### 2. 批量分诊

- `llm_batch_triage(scan_id, fingerprints…)`：默认上限 20 条（error>warning>info、路径序），提示模型输出 JSON 数组 `{fingerprint, assessment, rationale}`；逐条校验 assessment 枚举；未知/缺失 fingerprint 在结果中标注 missing。
- 验收 E2：解析/校验单测（含多余与缺失条目）；调用前后 findings/基线不变；UI 列表展示并标注「非确定性参考 · 不参与门禁」。

### 3. taint 候选规则

- 候选提示词允许 `taint:` 块（sources/sinks/sanitizers，tree-sitter query + @match）；校验沿用 `validate_candidate`（编译通过、`custom/` 前缀、id 唯一）。
- 验收 E3：合法 taint 候选 valid=true 并可保存；无 @match/编译失败返回 valid=false + errors。

### 4. 回归

- 验收 E4：既有 LLM 5 单测、无头 7、前端 5 全绿；新增测试后 clippy 0 警告。

## 排除项

流式用于候选规则生成（v3.2 仅分诊流式）、自动重试、批量上限配置 UI、LLM 结果持久化。

## 结论（2026-11-09，已实施）

- 流式：`parse_sse_data_line` + `SseParser`（按字节缓冲跨 chunk、UTF-8 安全、`[DONE]`/事件行/非 JSON 行跳过）+ `complete_stream`；命令 `llm_triage_stream` 经 `llm://delta`（requestId 过滤）增量发射；UI 流式面板实时渲染并在结束后回落完整文本。
- 批量：`llm_batch_triage`（空输入按 severity/path 确定性取前 20；显式输入去重、>20 拒绝）；JSON 数组解析 + assessment 枚举校验 + 缺失/多余指纹标记；只读不改库（headless 断言）；UI 列表常驻「非确定性参考 · 不参与门禁」。
- taint 候选：提示词允许 `taint:` 三选一；`validate_candidate` 追加每条 taint query 的 `@match` 捕获校验；合法 taint 候选可保存生效。
- 验证：clippy 0；workspace 全绿（LLM 单测 9 / 无头 8 / 引擎 31 / 集成 10 / 规则套件 / CLI 6）；tsc + vitest 6 + build；`docs/llm.md` 同步。
- 备注：UI 增量载荷改为 `in` 收窄（移除内联对象断言，符合风格规则）。
