---
status: done
---

# Task Spec：规则误报 P25

task-id: `rule-fp-fn-p25`

## 背景

基于 `codescan-findings-22`（1.6.0 扫描结果）复核后，高噪声 Rust 规则仍有两类上下文误报：

- `rust/fs-read-unbounded` 在已经用 `metadata.len()` 做大小保护后仍报告整读。
- `rust/fs-read-unbounded`、`rust/unwrap-in-pub-fn`、`rust/panic-in-library`、`rust/let-underscore-ignore` 在 `#[cfg(test)] mod tests` 内报告测试代码；测试代码中的 unwrap/读 fixture/丢弃返回值不应与生产代码同权重。

## 范围

- `rust/fs-read-unbounded` 增加函数内 `metadata.len()` 比较过滤；有大小保护时不再报告。
- 上述四条 Rust 规则跳过名为 `tests` 的模块。
- 补 bounded read 与 tests module fixture。

## 验收

- [x] 有 `metadata.len()` 边界检查的整读不命中 `rust/fs-read-unbounded`。
- [x] `#[cfg(test)] mod tests` 内的整读/丢弃返回值不命中对应 Rust 启发式规则。
- [x] 无大小保护的整读仍命中。
- [x] 规则套件通过；当前工作区对同一公开仓扫描 `501 → 490`。
- [x] 全量门禁通过。

## 排除项

- 不是通用路径级测试排除；仅跳过名为 `tests` 的 Rust 模块。
- `tests/` 独立测试文件仍按普通源码扫描。
- `rust/fs-read-unbounded` 的 TOCTOU/其他大小检查形式仍可能漏判或误报。
