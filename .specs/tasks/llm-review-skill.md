---
status: done
---

# Task Spec：LLM 源码审查内置技能包与独立复核

task-id: `llm-review-skill`

## 目标

在现有 LLM 源码审查（单轮、固定提示词）上增加两层可靠性机制：随二进制内置且版本化的审查技能包，以及默认开启、可关闭的独立复核轮。产出仍是展示层建议，不进入发现、基线、门禁与 SARIF。

## 现状（证据）

- `llm.rs::review_system_prompt` 是固定字符串，只声明三个维度与输出格式；没有技能资产、版本标记或复核环节。
- `parse_review` 只做结构校验（路径、行号区间、枚举、去重）；模型给出的 `confidence` 未经任何独立判断。
- `review_batched` 按 4 文件 / 32 KiB 分批，每批一次 `complete` 调用；没有额外请求。

## 范围

1. **内置技能包**
   - 新增 `src-tauri/skills/llm-review/`：`SKILL.md` 与 references（假阳性闸门、严重级/置信度校准、安全/业务逻辑/质量检查表）。
   - 内容经 `include_str!` 编入二进制，随应用版本化；不联网加载、不执行技能内容。
   - 审查系统提示词 = 注入边界声明 + 技能包；保持现有 JSON 数组契约、字段与行号语义。
2. **独立复核**
   - 默认开启；审查卡提供开关，随 `llm_review_workspace` 下发 `verify`。
   - 每批猎取完成后，若本批有建议且复核开启，用同一源码片段与候选清单发起一次独立复核请求；复核者不得信任候选断言。
   - 判定枚举 `confirmed | needs_validation | rejected`：`rejected` 从结果剔除并计数；`needs_validation` 保留并标注；缺失、越界、非法判定与调用失败一律降级为 `needs_validation`，不得使审查失败。
   - 复核关闭时不发第二次请求，建议的 `verification` 为 `null`。
   - 跨页去重合并时 `confirmed` 优先于 `needs_validation`。
3. **结果与展示**
   - `ReviewSuggestion` 增加 `verification`（null / confirmed / needs_validation）；`ReviewResult` 增加 `rejected` 与 `verification_enabled`。
   - UI：开关「独立复核」；列表徽标「已复核 / 待验证」；摘要展示复核过滤数与确认/待验证计数；复核关闭时不显示复核徽标。
   - 导出 Markdown / JSON 与单条复制包含复核状态；JSON 与后端字段一致。
4. **失败语义**
   - 复核 HTTP/解析失败：该批候选全部标 `needs_validation`，审查继续；不新增持久化写入。
   - 猎取失败语义不变（含 thinking 截断二分与分页）。

## 验收标准

- [ ] Rust 单测：审查系统提示词包含技能包版本标记、注入边界与三维度检查表；复核提示词包含候选清单与不可信数据声明。
- [ ] Rust 单测：复核解析按 index 映射；`rejected` 仅由显式判定产生；缺失/越界/非法判定/重复 index → `needs_validation`；整体解析失败降级而非报错。
- [ ] Rust 集成（本地 HTTP）：复核开启时每批先猎取后复核，`confirmed`/`needs_validation` 写入建议、`rejected` 剔除并计数；关闭时每批仅一次请求且 `verification = null`。
- [ ] Rust 单测：复核调用失败（HTTP 5xx / 非 JSON）时审查成功返回，候选全部 `needs_validation`，`rejected = 0`。
- [ ] Rust 单测：跨批去重时同键 `confirmed` 覆盖 `needs_validation`。
- [ ] 前端测试：默认开启复核，请求带 `verify: true`；关闭后带 `verify: false`；列表渲染徽标、摘要渲染复核计数。
- [ ] 前端测试：跨页合并时 `confirmed` 优先；导出 Markdown / JSON 含复核状态。
- [ ] 无写入路径：建议不进入 findings、基线、门禁与 SARIF。
- [ ] 文档：Domain §3b、`docs/llm.md`、`docs/en/llm.md`、`CHANGELOG.md` 同步。

## 排除项

- 不引入外部或用户安装的 `SKILL.md`，不联网加载技能。
- 不做多轮猎取、跨文件污点或漏洞可利用性验证。
- 不改建议的保存、历史与门禁语义，不新增持久层。
- 不改变现有分批、截断二分的输入选择与分页语义。

## 关联

- Domain：`.specs/domain/scan-engine.md` §3b
- 现有实现：`llm.rs` review_*、`commands.rs::llm_review_workspace`、`ScanView.tsx`、`reviewExport.ts`、`llmStore.ts`
