---
status: done
---

# Task Spec：质量补强（度量测试 / fixture 多重集 / 历史下钻 / CI）

task-id: `codescan-quality-pass`

## 背景与目标

v1–v3 验收后的四项工程缺口（事实已核对）：①`metrics.rs` 无单测，非 TS 语言的度量档位（函数/决策/块/不可达节点名）无任何断言，写错即静默失效；②规则套件期望集合用 `BTreeSet<(rule,file,line)>`，同规则同行多命中被塌缩；③历史页「查看」仅显示发现数量，无法浏览发现详情；④仓库无 CI。

## 范围与验收

1. **非 TS 度量已知答案测试**：`metrics.rs` 为 Python/Java/Go/Rust 各加单测，断言圈复杂度精确值、嵌套深度精确值、终止语句后不可达位置；任一语言档位名字错误必须导致测试失败。（验收：新增测试全绿，且临时改错一个档位名会被测试红灯——以代码审查确认断言粒度。）
2. **fixture 多重集**：期望与实际改为计数比较（同 (rule,file,line) 出现 N 次即要求 N 次）；v1 Task Spec A2 与 `docs/rules.md` 措辞同步为「多重集精确相等」。（验收：构造重复命中用例通过；漏/多任一计数触发失败。）
3. **历史下钻**：历史页加载某扫描后展示发现列表（严重级/路径/行/规则）并可点选查看详情（消息、命中文本、污点链）；不改变历史数据。（验收：vitest 断言「查看 → 列表 → 详情含 flows」。）
4. **CI**：GitHub Actions（macos-latest）：pnpm frozen install → 前端 typecheck/test/build → `cargo clippy -D warnings` → `cargo test --workspace`。（验收：workflow 文件存在且步骤与本地门禁一致；语法经 `act`? 不引入；以 YAML 结构审查为准。）

## 排除项

打包签名/公证、远端 SARIF 上传、指标阈值策略（另立任务）。

## 失败语义

- 度量断言失败即修复 Profile 档位（不得放宽断言）。
- 多重集计数不匹配时，错误信息必须同时给出缺失/多余的 (rule,file,line,count)。

## 结论（2026-11-09，已实施）

1. **度量测试**：metrics.rs 新增 13 条已知答案单测（Python/Java/Go/Rust × 圈复杂度/嵌套/不可达 + 长度抽样）。测试立刻暴露并修复两个真实缺陷：Java `block` 子节点直接挂语句（移除错误的 `block_statement` 包装假设）；Rust `return` 被 `expression_statement` 包裹导致不可达检测完全失效（补内层终止表达式规则）。
2. **fixture 多重集**：改为 `BTreeMap<(rule,file,line), count>` 计数比较，错误信息含期望/实际次数；v1 Task Spec A2 与 docs/rules.md 同步。
3. **历史下钻**：历史页可查看某次扫描的发现列表与详情（含污点链）。顺带修复两处 IPC 载荷契约缺陷：`scan://finished` 的 `scan_id`（serde snake_case，前端原按 camelCase 读取 → 真实应用完成事件不会触发）与 `ScanSummary.started_at_ms` 等字段；新增历史下钻 vitest。
4. **CI**：`.github/workflows/ci.yml`（macos-latest，pnpm frozen install → typecheck → test → build → clippy -D warnings → cargo test --workspace，含 cargo/pnpm 缓存）。

验证：clippy 0 警告；workspace 全绿（引擎单测 25 / 集成 8 / 规则套件 / 无头 7 / LLM 5）；前端 tsc + vitest 5 + build。
