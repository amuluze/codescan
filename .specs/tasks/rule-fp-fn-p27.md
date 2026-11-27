---
status: done
---

# Task Spec：重复块测试代码降噪 P27

task-id: `rule-fp-fn-p27`

## 背景

当前扫描 `metric/duplicate-block` 的 27 条发现主要来自：

- `llm.rs` 的 `#[cfg(test)] mod tests` 测试脚手架重复（13 条）。
- `App.test.tsx` 等测试文件默认状态重复。
- 顶层 `tests/*.rs` 集成测试重复。

这些重复对生产代码质量判断价值低，且淹没真实重复块。

## 范围

- `metric/duplicate-block` 新增 `options.skip_tests`（内置默认 `1`）。
- 语料增加测试代码行区间：测试文件整文件、Rust `mod tests` 精确行区间。
- 重复窗口完全落入测试区间时跳过。
- 补 `mod tests` 重复 fixture。

## 验收

- [x] `mod tests` 内重复代码不产生 `metric/duplicate-block`。
- [x] 测试文件与顶层 `tests/*.rs` 不产生重复块。
- [x] 现有 `metric-duplicate-block` 正例 fixture 仍命中。
- [x] 当前工作区对同一公开仓扫描 `457 → 430`。
- [x] 全量门禁通过。

## 排除项

- 嵌套测试 fixture（`tests/fixtures/...`）仍按普通文件扫描。
- `skip_tests` 仅对 duplicate-block 生效；其他度量仍可按自定义配置扫描测试。
