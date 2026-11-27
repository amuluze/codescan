---
status: done
---

# Task Spec：自扫描发现分诊与修复

task-id: `codescan-self-scan-remediation`

## 背景与目标

用户以桌面应用扫描本仓库（engine 1.6.1，根 = 仓库根，导出 `codescan-findings-24.json/md`，425 条）并要求「分析修复」。目标：逐条判定真阳性 / 设计内 / 规则近似，修复真阳性，消除设计内的噪音来源，并为无法消除的设计内发现给出显式处置（基线）。

补充事实：同一代码用当前树内引擎 1.6.0 扫描得 515 条，用户导出是 1.6.1 引擎的子集（1.6.1 已对测试代码与「终止语句后同行注释」等场景降噪）。以当前引擎结果为准做修复，1.6.1 独有的判定差异不作为验收对象。

## 分诊结论

| 类别 | 数量（导出） | 判定 | 处理 |
|---|---|---|---|
| `crates/engine/tests/fixtures/**` 下规则命中 | 290 | 设计内：规则测试语料故意含缺陷，由 `rule_suite` 精确断言 | 写入 `.codescanignore`（`crates/engine/tests/fixtures/`），自扫描不再产出 |
| `rust/unwrap-in-pub-fn`（锁定 `Mutex`、JSON 序列化） | 39 | 真阳性：公共入口可能 panic | `lock_recovering`（中毒取回 guard）；`active_provider_json` 返回 `Result`；`insert_scan` 去掉两处 `unwrap` |
| `rust/let-underscore-ignore` | 24 | 混合：`store.set_secret` 丢弃错误会静默丢密钥（真阳性）；事件发送、临时目录清理属可恢复操作 | 密钥迁移改为单事务全部成功或全部回滚；事件/清理失败输出 stderr；`overrides.set` 记录被跳过的条目 |
| `rust/fs-read-unbounded` | 24 | 真阳性：读取前无上限 | 新增 `codescan_engine::fsutil::{read_bytes_limited, read_to_string_limited}`，全部读取点改为显式上限（扫描 1 MiB、规则/pack 1 MiB、密钥 64 KiB、SARIF/基线 64 MiB） |
| `rust/taint-fs-write-param` | 14 | 数据流真实但属功能本身（用户选定目录的导出、临时语料、密钥落盘、CLI `--out`）；其中 `export_sarif` 接受任意路径属真阳性 | 导出/临时语料/密钥写入保留（写路径已由 Rust 校验）；`export_sarif` 改为「目录 + Rust 生成文件名」，与 `export_report` 同一边界 |
| `rust/panic-in-library` | 5 | 真阳性（`unreachable!` 位于可由调用方到达的分支） | 取消分支返回 `Err(Cancelled)`；重试循环改为显式计数循环；`language::parse` 返回 `Result`，解析失败记为逐文件事件 |
| `metric/*`（复杂度/长度/嵌套/重复块） | 46 | 真阳性（可维护性） | 见下节实施 |
| `text/todo-debt`、`text/aws-access-key` | 4 | 混合：注释中的字面量属噪音；测试内假密钥属设计内 | 注释改写；测试假密钥改为运行时拼接 |

## 实施

- **Tauri 命令层**：`lock_recovering` 统一取锁（中毒取回，不 panic）；`ActiveRegistration` 守卫在扫描线程 panic 时释放「正在扫描」登记；`store.migrate` 以 `PRAGMA table_info` 判断列存在，其余错误不再被吞掉；`Store::transaction` 用于旧版 Provider 迁移与 Profile 删除；事件发送失败记录 stderr；临时文件替换共用 `replace_file`。
- **引擎/CLI**：`fsutil` 有界读取；`scan_file` 拆分读取/分类/分析，未知扩展名走 `scan_text_only`；`collect_import_aliases` 按语言拆分；`run_ast_rule` 抽出 `match_ranges`/`capture_match`/`metavariables_hold` 等；`run_pack` 拆成每子命令一个函数；CLI 读取点加显式上限。
- **引擎度量重构**（行为保持）：`guarded_walk`/`guarded_call_walk` 合并为 `GuardWalk` + 访问器；`collect_flow`/`collect_constants_in`/`collect_loop_flow` 拆出 if/label/loop/do/absorb 子步骤；`extract_imports` 按语言拆分；`parse_rule_set` 按字段分组校验；`metrics::span_of`/`named_children` 与 `types.rs` 去重。
- **前端视图**：六个组件按「页面骨架 + `XxxView.parts.tsx` 展示区段/`useXxx` hook」拆分，`data-testid`/aria/文案/class 不变；`DESIGN.md` 同步组件落点。
- **自扫描噪音**：`.codescanignore` 忽略规则 fixture；`scan.rs` 注释去掉债务标记字面量；`metric/unreachable-code` 跳过 Rust 的 `line_comment`/`block_comment`（行尾注释不再被当成不可达语句），新增 `metric-unreachable-code/dead.rs` fixture。

## 验收

- [x] `pnpm check`（仓库完整门禁：notices、typecheck、vitest、前端 build、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace`）：全绿，clippy 0 警告。`rule_suite` 的 fixture 精确断言证明引擎重构未改变规则/污点语义。
- [x] 重新自扫描（根 = 仓库根，当前引擎 1.6.0）：**515 → 91**，`(rule_id, path, match_text)` 多重集差分 424 条消失、**0 条新增**（已消化重构自身引入的 4 条重复块与 5 条注释误判）。
- [x] fixture 命中为 0；`commands.rs`/`lib.rs`/`manager.rs`/`store.rs`/`regression.rs`/`scan.rs`/`rules.rs`/`types.rs`/`language.rs`/CLI 与全部视图的规则类与度量类命中为 0（仅剩下文列出的设计内写入）。
- [x] `metric/unreachable-code` 的 Rust 行尾注释误判已修（引擎缺陷 + `dead.rs` fixture），残余 91 条不再包含该项。

## 排除项与残余

最终 91 条全部落在四类，均不属于本次「修复真阳性」的范围：

1. **设计内写入**（11 条 `rust/taint-fs-write-param`，建议在应用内设为基线）：`export_report` 写前端生成的 Markdown/JSON、`save_custom_rules` 写用户 YAML、`test_custom_rule`/`regression` 写临时语料、`packs::trust_key` 写公钥、CLI `--out`/`--save-baseline` 写用户指定文件、两处测试 helper 的 `fs::write`。
2. **测试代码**（`tests/**`、`#[cfg(test)]` 的 `llm.rs`/`taint/mod.rs`、`App.test.tsx`、`llmStore.ts`）：规则命中与度量命中。1.6.1 引擎对测试代码豁免，用户的导出中本就没有这些；本次不改测试代码。
3. **未知扩展名文本**（`builtin.yaml`、`docs/*.md`、`.specs/*.md`、`*.svg`/`*.plist`/`*.txt`/`tauri.conf.json`）的 `text/todo-debt`、`text/http-url`：1.6.0 对未知扩展名跑文本规则，1.6.1 未报。
4. `metric/duplicate-block` 的跨文件 11 行近重复（`FindingsView.parts.tsx` ↔ `HistoryView.parts.tsx` 的发现行）低于 12 行窗口，未处理。
