---
status: done
---

# Task Spec：规则库质量批次 1

task-id: `rule-library-quality-v1`

## 背景

路线图第 2 项需要的不只是规则数量，而是可维护、可验证的规则供给。本批次先建立质量门槛，并补充 4 条高价值规则。

## 规则质量门槛

- 每条规则至少一个正例 fixture；负例放在同一 fixture 目录，runner 精确多重集判定会拒绝未标注命中。
- 规则消息只表述结构构造，不声称可利用性；taint 规则沿用“可能到达”口径。
- severity/dimension 由规则声明；规则 id 语义变更必须走版本推进。
- 规则套件全量通过；新增规则不得让既有 fixture 产生额外命中。

## 本批次规则

- `ts/prototype-pollution`：向 `__proto__`/`constructor`/`prototype` 写入属性。
- `ts/child-process-string`：`child_process.exec/execSync` 以字符串命令执行。
- `py/requests-verify-false`：requests 调用显式 `verify=False`。
- `py/mark-safe`：Django `mark_safe` 跳过 HTML 转义。

## 验收标准

- [x] 4 条规则均有正例 fixture，且同目录负例不产生未标注命中。
- [x] 规则套件全量通过。
- [x] README 规则计数与维度计数同步（94 条；security 80）。
- [x] CHANGELOG 记录新增规则。
- [x] `task check` 通过。

## 排除项

- 不在本批次引入规则包机制（路线图 3）。
- 不追求一次性覆盖全部 CWE/框架；后续按覆盖矩阵继续补充。
- 不改规则 id 版本语义；新增规则使用新 id。
