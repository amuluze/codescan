# LLM 辅助层（v3.2）

Provider 形态复刻 axiom：OpenAI-compatible 与 Anthropic-compatible 两类，BYO API Key，无内置遥测。

## 配置

设置页填写 Provider kind、Base URL、Model 与 API Key：

- API Key 仅写入 `~/.codescan/codescan.db` 的 `secrets` 表（Rust 独占读写）；命令面只暴露「已配置/未配置」，不存在明文回读接口。
- 请求只发往你配置的 Base URL；OpenAI 走 `{base}/chat/completions`（Bearer），Anthropic 走 `{base}/v1/messages`（`x-api-key` + `anthropic-version`）。
- 超时：非流式 60s，流式 120s；错误信息不含密钥。

## 能力

### 1. 单条分诊（参考）

在发现详情点击「LLM 分诊」，模型返回 `true-positive | likely-false-positive | uncertain` 与理由。结果仅展示，**不写入发现、不参与基线/门禁**（Domain §3a/§5.3）。

### 2. 流式分诊

发现详情点击「流式分诊」，Rust 侧经 SSE 增量解析后逐片 emit `llm://delta` 事件：

```json
{ "requestId": "<本次请求 id>", "text": "增量文本" }
```

UI 只接收与当前 `requestId` 匹配的增量并实时渲染，命令结束时返回完整文本。解析器兼容 OpenAI `choices[0].delta.content` 与 Anthropic `content_block_delta.delta.text`，容忍 `[DONE]`、空 delta、非 JSON 行与网络分片跨行；多字节 UTF-8 在分片边界不会损坏。失败（无 Key、网络、HTTP 非 2xx、读取流失败）返回可读错误且不含密钥。

### 3. 批量分诊

发现页点击「批量分诊」，命令 `llm_batch_triage(scan_id, fingerprints)` 一次分诊至多 20 条：

- UI 传入当前过滤列表中的前 20 个指纹；传入空数组时由后端按 `error > warning > info`、路径、位置、规则 id 的确定性顺序取前 20 条。
- 提示词要求模型只输出 JSON 数组 `[{ "fingerprint", "assessment", "rationale" }]`，assessment 枚举同单条分诊。
- 解析逐条校验 assessment 枚举；模型漏掉的请求指纹与多返回的未知指纹都以 `missing: true` 标注，不丢弃任何一侧。
- 调用前后不写 findings、基线或历史；结果仅在发现页列表展示（带「非确定性参考 · 不参与门禁」标注）。

### 4. 候选规则回写

点击「生成候选规则」，模型产出一条 `custom/` 前缀的规则 YAML；Rust 侧做编译校验（query 语法、枚举、id 唯一、与内置不冲突）。仅当校验通过且你显式点击「保存为自定义规则」后，规则才写入 `~/.codescan/rules/custom.yaml`，并在下一次扫描生效。

候选规则体三选一：

- `pattern`：tree-sitter query，必须包含 `@match` 捕获；
- `text`：Rust regex，`language` 必须为 `"*"`；
- `taint`：`sources` / `sinks` / `sanitizers` 三组 tree-sitter query，`language` 必须为具体语言，`sources` 与 `sinks` 至少各一条，每条 query 必须包含 `@match` 捕获。语义为「不可信数据可能到达 sink」的同文件过程内近似，不声称跨文件、堆别名或字段敏感证明（Domain §4a）。

候选校验复用 `llm::validate_candidate`：解析、`custom/` 前缀、taint 端点 `@match`、整体编译均通过才 `valid=true`；否则返回 `valid=false` 与逐条 errors。

## 纪律与边界

- LLM 产出永不进门禁；只有保存后的确定性规则参与扫描与基线。
- 分诊（单条/流式/批量）均为展示层参考，不写发现、基线、历史或门禁差异。
- 失败可恢复：无密钥、网络错误、模型输出不可解析均返回可读错误，扫描/历史/基线不受影响。
