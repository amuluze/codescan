# Plan：llm-review-skill

对应 Task Spec：`.specs/tasks/llm-review-skill.md`；领域约束：`.specs/domain/scan-engine.md` §3b（v8）。本文件只描述实现路径。

## 设计闸门

调用方用法（后端）：

```rust
review_batched(config, key, files, verify, |done, total| { ... })
    -> Result<(Vec<ReviewSuggestion>, dropped: u32, rejected: u32), String>
```

- 技能包在编译期拼入系统提示词，运行期无文件读取、无网络拉取。
- 复核粒度 = 猎取批次（≤4 文件 / 32 KiB）。复核输入复用该批 `<file>` 片段 + 候选 JSON 列表；输出 `[{index, verdict, reason}]`，`index` 为 1-based。
- `verification: Option<String>` 存在 `ReviewSuggestion` 上；`rejected` 与 `verification_enabled` 存在 `ReviewResult` 上。复核关闭时字段为 `null` / `false`。
- 复核失败只降级不改调用链：`verify_review` 内部吞掉 HTTP/解析错误，把该批候选统一标 `needs_validation`。

## 技能包布局

```text
apps/desktop/src-tauri/skills/llm-review/
├── SKILL.md                      # 协议：边界、工作流、证据要求、输出契约
└── references/
    ├── false-positives.md        # 配置值/框架缓解/测试代码/不可达代码闸门
    ├── severity.md               # 严重级与置信度校准
    ├── security.md               # 注入/鉴权/密钥/反序列化/SSRF/路径/加密/配置
    ├── business-logic.md         # 归属/状态机/金额库存配额/幂等/事务/竞态
    └── quality.md                # 吞错/并发/泄漏/死代码/可维护性
```

- `llm.rs` 用 `concat!(include_str!(...), ...)` 组合；版本常量 `REVIEW_SKILL_VERSION = "1"` 渲染进提示词，并作为单测断言锚点。
- 复核提示词只加载 `SKILL.md` + `false-positives.md` + `severity.md`，避免重复发送三维度清单。

## 实施步骤

1. **技能包与提示词**：新增 assets；重写 `review_system_prompt()` 为组合结果；抽出 `review_files_block()` 供猎取/复核共用；新增 `review_verify_system_prompt()` 与 `review_verify_user_prompt()`。
2. **复核内核**：新增 `parse_verification(text, count)`（index 映射、非法/缺失→`needs_validation`、重复 index 后者忽略）与 `verify_review()`（调用 `complete`，失败整批降级）；`ReviewSuggestion` 加字段。
3. **批次聚合**：`review_batched` 增 `verify` 参数与 `rejected` 返回；猎取成功后执行复核；末次去重改为 `merge_review_suggestions`，同键 `confirmed` 覆盖 `needs_validation`；进度在复核完成后推进。
4. **命令/API**：`llm_review_workspace` 增 `verify: Option<bool>`（缺省 `true`），返回新字段；`platform.ts` 类型与 `api.llmReviewWorkspace` 透传；`llmStore.runReview` 追加 `verify` 形参、累计 `rejected`、合并优先 `confirmed`。
5. **UI**：`ScanView` 增加受控开关（默认开、运行中禁用）；`runReview` 调用传值；摘要追加「复核：确认 N / 待验证 M / 过滤 K」；列表头加 `.verify-chip`；关闭时不渲染徽标。样式沿用现有 chip 规范。
6. **导出与 Copy**：`reviewExport.ts` 增加「复核：已复核/待验证（未复核时省略）」，Markdown 与单条复制一致；JSON 直接序列化新字段。
7. **文档**：`docs/llm.md`、`docs/en/llm.md` 增加技能包/复核说明与成本提示；CHANGELOG 记录；Task Spec frontmatter 完成时置 `done`。
8. **测试**（先写失败用例，再实现到绿）：
   - `llm.rs`：提示词包含版本/注入边界/三维度；复核解析映射与降级；本地 `TcpListener` 服务器断言开启时 2 类请求序列、关闭时 1 类；5xx 降级；跨批 confirmed 覆盖。
   - `App.test.tsx`：默认 `verify: true`；关闭后 `verify: false`；徽标与复核汇总；导出含复核状态。
   - 真实表面：vite + 注入 store 数据截图，检查徽标/开关/摘要在 1280 与 640 宽度下的布局。

## 验收映射

| 验收 | 证据 |
|---|---|
| 技能包/复核提示词 | `review_*_prompt` 单测 |
| 解析与降级 | `parse_verification` / `verify_review` 单测 |
| 开启/关闭请求序列 | 本地 HTTP 集成（`review_batched`） |
| 复核失败不失败审查 | 5xx 本地 HTTP 单测 |
| 跨批 confirmed 优先 | `merge_review_suggestions` 单测 |
| 前端开关/徽标/汇总/导出 | `App.test.tsx` + 浏览器实拍 |
| 不写入门禁 | 无新增写入路径；复查 `llm_review_workspace` 返回链 |

## 风险与回滚

- **误杀**：`rejected` 只在复核给出显式否定证据时生效；解析/调用异常一律保留为待验证，避免丢建议。
- **成本**：默认开启使每批最多多一次请求；开关与摘要中的复核计数让成本可见。若需回滚，前端关开关即可恢复单轮路径，后端字段保持向后兼容。
- **合并语义**：跨页同键可能先待验证后确认；合并函数显式优先 `confirmed`，避免顺序依赖。
