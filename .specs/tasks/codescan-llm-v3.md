---
status: done
---

# Task Spec：Codescan v3 —— LLM 辅助层（分诊 + 候选规则回写）

task-id: `codescan-llm-v3`

## 背景与已定决策

汇报结论：LLM 层只做「发现未知/分诊误报/生成候选规则」的辅助，产出**不进门禁**；只有经校验并显式保存的确定性规则才影响后续扫描（Domain §3a/§5.3）。用户已拍板：

1. **Provider 形态复刻 axiom**：OpenAI-compatible 与 Anthropic-compatible 两类，BYO API Key，无内置遥测。
2. **密钥存储**：SQLite `secrets` 表，Rust 独占；命令面只暴露「已配置/未配置」。
3. **结果面**：LLM 可生成候选规则并**写回**自定义规则集（经编译校验与用户显式保存）。

v3.1 语言范围：候选规则生成对全部 6 语法族开放；分诊对任意发现开放。

## 范围

### Rust（可无头测试）

- `secrets` 表与 meta KV：读写密钥（只写/删/状态）、Provider 配置（kind/base_url/model）持久化。
- LLM 客户端：openai（`POST {base}/chat/completions`，Bearer）与 anthropic（`POST {base}/v1/messages`，`x-api-key` + `anthropic-version`）；60s 超时；响应解析纯函数化。
- 命令：
  - 配置：`set_provider_config` / `get_provider_config` / `set_api_key` / `clear_api_key`（永不回传明文）。
  - 分诊：`llm_triage(scan_id, fingerprint)` → 结构化建议（assessment/rationale），仅展示。
  - 候选规则：`llm_generate_rule(scan_id, fingerprint)` → 模型产出完整规则 YAML，Rust 侧编译校验并返回 `{yaml, valid, errors}`；`save_custom_rule(yaml)` → 与既有自定义规则合并再校验（id 唯一、`custom/` 前缀、编译通过）后追加持久化。
- 纪律：分诊与候选生成不写 findings、不影响基线；候选规则未保存前不参与任何扫描。

### UI

- 设置页：Provider kind（openai/anthropic）、base_url、model、API Key（只写；已配置时显示状态与「清除」）；保存/读取。
- 发现详情：`LLM 分诊` 按钮 → 展示建议并显式标注「非确定性参考 · 不参与门禁」；`生成候选规则` → 展示 YAML + 校验结果 + `保存为自定义规则`（仅 valid 可保存）。
- 规则页：自定义规则校验错误展示沿用既有通道。

## 验收标准（全部可客观判定）

- **C1 密钥边界**：设置密钥后状态为已配置、清除后未配置、重开数据库后状态保持；命令面不存在返回明文的接口（代码审查 + 状态端点断言）；错误消息不含密钥（单测注入含密钥的错误路径）。
- **C2 Provider 配置**：保存/读取 kind/base_url/model 往返一致；未配置密钥时调用 LLM 命令返回可读错误且不发起请求（单测）。
- **C3 请求构造**：两类 provider 的请求体、鉴权头、响应解析（成功/错误 JSON）为纯函数并有单测；不依赖网络。
- **C4 分诊纪律**：triage 返回结构含 assessment ∈ {true-positive, likely-false-positive, uncertain}；调用前后 scans/findings/基线表不变（命令层断言）；UI 文案含「非确定性参考」。
- **C5 候选规则回写**：非 YAML/编译失败/id 非 `custom/` 前缀/id 冲突 → 拒绝并返回错误；合法候选保存后出现在自定义规则目录且下一次扫描生效（引擎层断言：Engine::with_rules 可加载）。
- **C6 UI**：vitest 覆盖设置表单保存、分诊面板渲染与「不参与门禁」标注、候选规则保存调用。
- **C7 失败可恢复**：无密钥/网络错误/超时 → 命令返回错误文本，扫描、历史、基线不受影响（单测）。

## 排除项

- 流式输出、批量分诊、自动修复、LLM 结果入门禁/基线、模型侧工具调用、嵌入式沙箱执行；提供方以外的平台集成（GitHub/GitLab）。

## 失败语义补充

- 模型返回非规则 YAML：`valid=false` + 错误列表，不阻断后续生成尝试。
- 自定义规则文件损坏：保存命令拒绝覆盖，返回逐条错误（与 Domain §4.4 一致）。
- API Key 缺失：所有 LLM 命令在发出请求前返回「未配置 API Key」。

## 结论（2026-11-09，已实施）

v3.1 落地。Rust：`secrets`/`meta` 表（密钥只写/删/状态，明文读取仅 Rust 内部）、Provider 配置（openai/anthropic 双形态）、`llm.rs` 纯函数请求构造/响应解析/分诊 JSON 解析/候选 YAML 提取/合并校验、60s 超时网络调用；命令面 7 个（配置 2、密钥 2、分诊/生成/保存 3）。UI：设置页（Provider + 只写 Key + 状态）、发现详情分诊面板与候选规则面板（显式「非确定性参考 · 不参与门禁」标注）。文档 `docs/llm.md`。

验证：clippy 0 警告；workspace 测试全绿——LLM 单测 5（C3 双 provider 体/解析、C5 校验与合并冲突）、无头 7（含 C1 密钥往返与持久化、C2 无密钥错误且不发请求、C7 网络失败可恢复）；前端 tsc + vitest 4（C6 设置保存/分诊标注/候选保存）+ vite build；`pnpm tauri dev` 启动冒烟通过。

已知边界：分诊与候选生成依赖真实 Provider（自动化仅覆盖错误路径与纯函数）；候选规则仅允许 pattern/text（taint 生成不支持）；网络重试与流式输出不在 v3.1。
