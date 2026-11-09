---
status: done
---

# Task Spec：Codescan v3.3 —— LLM 候选规则自动化回归

task-id: `codescan-llm-v33`

## 目标与边界

候选规则生成后自动做一次**隔离回归**：在种子发现所在文件的临时语料上，仅启用候选规则（禁用全部内置规则），按规则套件语义（标注行 ↔ 实际命中）校验；结果仅展示，可保存夹具到数据根。不改变门禁/基线。

## 范围与验收

- Rust 命令 `llm_candidate_regression(scan_id, fingerprint, candidate_yaml)`：
  1. `validate_candidate`（沿用：编译、`custom/` 前缀、id 唯一）。
  2. 从扫描记录的 root + finding.path 读取源文件；复制到临时目录（保持文件名/扩展名）。
  3. 以「发现起始行」为期望：按语言注释语法在目标行上方插入 `ruleid: <候选 id>` 标注。
  4. 用 `Engine::with_rules(Some(candidate), disabled=全部内置 id)` 扫描临时目录；比较期望/实际 `(rule,file,line)` 多重集。
  5. 返回 `{valid, pass, expected: [{line}], actual: [{line, message}], failures: []}`；可选 `save_fixture: bool` 时将标注文件写入 `~/.codescan/rules/tests/<候选 id>/`。
- 验收 G1：命中种子行的候选 → pass=true，actual 含期望行。
- 验收 G2：不命中（例如仅匹配其他构造）→ pass=false，failures 可读。
- 验收 G3：非法候选 / 源文件缺失 → 可读错误或 valid=false，不 panic。
- 验收 G4：UI 候选面板新增「运行回归」按钮与结果（通过/未通过、期望/实际行、失败原因）；vitest 覆盖。
- 验收 G5：只读纪律（调用前后 findings/基线不变）；既有 LLM/无头/前端测试全绿。

## 排除项

自动修复、自动保存并纳入内置规则集（仅保存到用户数据根）、跨文件语料、多发现批量回归。

## 结论（2026-11-09，已实施）

新增 `regression.rs`：候选校验 → 取扫描 root 与 finding.path（含路径穿越防护）→ 临时目录复制源文件并按语言在发现起始行上方插入 `ruleid:` 标注 → 以「禁用全部内置规则 + 仅候选规则」隔离扫描 → `(rule,line)` 多重集比较（漏报/过度匹配报告）→ 可保存夹具到 `~/.codescan/rules/tests/<rule id>/`。命令 `llm_candidate_regression(scan_id, fingerprint, candidate_yaml, save_fixture)`；UI 候选面板新增「运行回归验证」与「保存夹具」，展示通过/未通过、期望/实际行与失败原因。

验收：G1 命中（pass=true，夹具保存）；G2 未命中（failures 可读）；G3 非法候选/缺失源文件（结构化错误，不 panic）；G4 vitest 覆盖回归按钮与结果渲染；G5 只读（调用前后 findings/基线不变，headless 断言）。验证：`cargo test -p codescan-desktop` 21 通过（9 单测 + 12 headless，+4）；workspace clippy 0；前端 7 测试 + tsc + build。
