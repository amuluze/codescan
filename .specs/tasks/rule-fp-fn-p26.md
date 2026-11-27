---
status: done
---

# Task Spec：规则路径排除 P26

task-id: `rule-fp-fn-p26`

## 背景

`codescan-findings-22` 与当前扫描显示，测试文件、文档与规则定义贡献大量低价值告警：

- Rust 启发式规则命中顶层 `tests/*.rs`（integration/rule_suite/headless）。
- `text/todo-debt` 命中文档、任务规格与规则定义中的“TODO/FIXME”描述。

## 范围

- pattern/text 规则新增 `exclude_paths`（工作区相对路径正则列表）。
- `rust/unwrap-in-pub-fn`、`rust/fs-read-unbounded`、`rust/panic-in-library`、`rust/let-underscore-ignore` 排除顶层 `tests/*.rs`。
- `text/todo-debt` 排除 docs、`.specs`、`.github`、规则定义 YAML 与顶层 `tests/*.rs`。
- 更新规则文档与集成测试。

## 验收

- [x] `src/read.rs` 命中 `rust/fs-read-unbounded`，同级 `tests/read.rs` 不命中。
- [x] 规则套件通过。
- [x] 当前工作区对同一公开仓扫描 `490 → 457`。
- [x] 全量门禁通过。

## 排除项

- `exclude_paths` 仅支持 pattern/text 规则；taint/inheritance/metric 暂不声明路径排除。
- 嵌套 fixture（`tests/fixtures/...`）仍按文件路径正常扫描。
