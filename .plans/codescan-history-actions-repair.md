# Plan：历史页查看、基线与门禁差异修复

对应 Task Spec：`.specs/tasks/codescan-history-actions-repair.md`。

## 1. 设计闸门

### 1.1 先确定调用方用法

`HistoryView` 只消费一个差异状态，不再分别判断扫描 id、空数组和 `null`：

```tsx
const { gate } = useHistoryStore();

<HistoryScanTable
  onOpen={(scanId) => runAction(() => openScan(scanId))}
  onSetBaseline={(scanId) => runAction(() => setBaseline(scanId))}
  onRefreshGate={(scanId) => runAction(() => refreshGate(scanId))}
/>
<HistoryGateDiff gate={gate} />
```

差异组件内部完整处理 `idle / loading / no-baseline / ready`；`idle` 不渲染，其余状态标题均含 `扫描 #scanId`。

### 1.2 状态契约与取舍

候选 A：保留 `gateNew`，另加 `gateScanId`、`gateLoading`、`noBaseline`。改动文字较少，但四个字段可组成“加载中同时有旧结果”等非法状态，不能从类型上消除本次问题。

候选 B（采用）：单一可辨识联合，旧 `gateNew` 完整替换，不留兼容别名：

```ts
type GateState =
  | { status: "idle" }
  | { status: "loading"; scanId: number }
  | { status: "no-baseline"; scanId: number }
  | { status: "ready"; scanId: number; findings: Finding[] };
```

错误仍由 `HistoryView` 现有 `error` 统一展示，不塞进 `GateState`；失败时仅当前请求可把差异恢复为 `idle`。发现下钻保留现有字段，新增 `openWarnings: string[]` 表达事件/规则集部分失败，避免为次要元数据再建一套结果模型。

`baseline` 扩成三态 `BaselineInfo | null | undefined`：`null` 只表示后端明确返回无基线，`undefined` 表示初始未知或写入/删除后刷新未确认。基线写入、历史删除成功后先置 `undefined` 再刷新；刷新失败保持 `undefined`，页面显示「基线状态未知」和准确错误，既不保留旧基线，也不伪装成未设置。

不新增 IPC、不缓存多条扫描差异、不引入 reducer 或请求库。

### 1.3 请求时序契约

`historyStore.ts` 模块内使用单调递增的 `openVersion`、`gateVersion` 表达用户动作意图序，而不是网络请求发起序；每次用户动作捕获版本，提交前再次比较。查看引发的自动差异继承查看动作记录的 gate 版本，不因为实际较晚发起而获得更新意图：

- 新「查看」：递增 `openVersion`，清空旧下钻；递增 `gateVersion` 并将差异置为 `idle`，记录本次查看对应的 gate 版本。
- 查看响应：发现必须成功才提交下钻；事件/规则集允许失败但生成 warning。仅最新 `openVersion` 可提交。
- 查看自动差异：仅当 `gateVersion` 仍等于本次查看记录的版本，且本次查看的 `openVersion` 仍为最新时才触发。任何更晚的查看、显式差异、基线切换或删除都会阻止旧查看启动自动差异。
- 新「门禁差异」：先提交 `{status:"loading", scanId}`；响应只允许最新 `gateVersion` 写入 `no-baseline` 或 `ready`。
- 「设为基线」的 IPC、`deleteScan` 的 IPC 一成功，立即递增 `gateVersion`、将差异置 `idle`、将基线置 `undefined`，再刷新基线/列表。写入失败时保持原差异和原基线；后续刷新失败时保持基线未知并给出区分于写入失败的错误。
- 仅最新请求的 reject 在完成必要状态恢复后继续抛出，由页面 `runAction` 显示；过期请求无论成功或失败都直接结束，不得覆盖新结果或弹出来自旧操作的错误。

请求版本只存在于内存，应用重启自然复位；测试无需把版本重置为指定值。

## 2. 实施步骤

### T1：先补 RED 交互与故障注入测试

修改 `codescan/apps/desktop/src/App.test.tsx`：

1. 在现有 `beforeEach` 中以合并式 `setState` 只重置 `useHistoryStore` 数据字段：`scans / baseline / openScanId / openFindings / openEvents / openRuleset / gate / openWarnings / loading`，其中 `baseline=undefined`。不得整对象 replace 或清除 action；内部单调版本无需归零。默认 `invokeMock` 补 `gate_diff=null` 与成功的 `set_baseline` 分支，单用例再覆盖，禁止依赖 `undefined` 冒充 IPC 结果。
2. 用两条扫描和手动 deferred Promise 覆盖查看、差异逆序完成，以及“查看自动差异 vs 更晚显式差异”交错。两次查看逆序完成时，除下钻外还必须断言自动差异归属最后一次查看的扫描。
3. 覆盖无基线、四种差异状态标题、事件/规则集单独 reject、基线写入失败保留旧差异、写入成功但信息刷新失败仍失效且显示基线未知，以及删除基线来源/差异目标/无关扫描后的统一失效。
4. 覆盖规则包 integrity 正常值、`undefined` 和空串，后两种都显示未记录；另覆盖 `ruleset={}` 显示快照未记录，明确 `enabled_rules=[]` / `packs=[]` 才显示 0 数量。
5. 三个行操作 reject 用例单独分批运行：先记录旧实现的 unhandled rejection/缺少 alert 为 RED，再由 T3 修复，避免未处理拒绝掩盖竞态断言。

修改 `codescan/apps/desktop/src-tauri/tests/headless_test.rs`：

1. 第一次设基线后，用第二 `rusqlite::Connection` 创建 `BEFORE UPDATE ON active_baseline` 触发器，触发器体为 `SELECT RAISE(ABORT, 'baseline switch blocked');`。第二次设基线走 `ON CONFLICT DO UPDATE`，会命中该触发器。
2. 第二次设基线必须失败；用 `SELECT COUNT(*) FROM baselines` 断言行数不变，并用 `store.current_baseline()` 断言活动基线仍是第一次的指纹。
3. 测试依赖临时数据库，无需测试专用产品 API。

检查：新用例在旧实现上因空态缺失、旧响应覆盖、旧差异残留和多插入一行而失败；不得通过放宽断言使其变绿。

### T2：重构 historyStore 的提交与失效边界

修改 `codescan/apps/desktop/src/stores/historyStore.ts`：

- 落地 `GateState`、两个请求版本、`openWarnings`。
- `openScan` 使用 `Promise.allSettled` 同时请求发现、事件、规则集；发现 reject 则整体失败，事件/规则集 reject 则保留对应空展示并生成明确 warning。提交后只有本次 `openVersion` 仍最新且 `gateVersion` 仍等于查看开始时的记录值，才启动自动差异。
- `load` 与 `deleteScan` 的列表/基线刷新使用 `Promise.allSettled`：扫描列表读取成功即提交；基线读取失败保持 `baseline=undefined` 并单独向页面抛错，不隐藏列表，也不把未知显示成未设置。
- `setBaseline` 写入失败不改变原差异/基线；写入成功后立即使差异失效并置 `baseline=undefined`，再读取信息。刷新失败抛出「基线已切换，但信息刷新失败」，不得报成写入失败或恢复旧差异。
- `deleteScan` 删除成功后立即使差异失效并置 `baseline=undefined`，再刷新列表和基线。基线来源、差异目标、无关扫描都采用同一失效路径；刷新失败保持基线未知。
- 删除旧 `gateNew` 及其所有调用方，不保留并行状态。

检查：T1 的竞态、失效和元数据用例通过；若基线信息刷新失败则操作报错，不能伪报成功。

### T3：历史页状态展示与统一错误出口

修改 `codescan/apps/desktop/src/views/HistoryView.tsx`、`codescan/apps/desktop/src/views/HistoryView.parts.tsx`：

- 用一个局部 `runAction` 清空旧错误并捕获三个行操作异常，写入已有 `role=alert`。
- 基线信息行区分 `undefined=基线状态未知`、`null=未设置基线` 和有信息三种状态。
- `HistoryGateDiff` 接收完整 `GateState`，渲染加载、无基线、无新增、新增列表及扫描 id。
- 下钻区域渲染 `openWarnings`；警告与主操作错误样式/语义区分。
- `ruleset` 缺少 `enabled_rules` 与 `packs` 字段时显示「规则集快照未记录」；只有字段明确为数组时才计算 0 数量。
- 规则包行追加 integrity；`undefined` 与空串都显示「integrity 未记录」，不显示伪造哈希或空白哈希位。
- 不调整无关表格、删除确认、SARIF 导出和样式体系。

检查：T1 全部前端用例通过；随后运行 `task typecheck`。

### T4：基线快照与活动指针同事务

修改 `codescan/apps/desktop/src-tauri/src/store.rs`：

- 保留现有指纹读取、排序、去重和 JSON 序列化逻辑。
- 复用 `Store::transaction()` 包裹 `INSERT INTO baselines` 与 `active_baseline` upsert，由闭包返回新 baseline id。
- 不改表结构、不迁移旧数据；SQLite 回滚保证失败时无新快照且原活动指针不变。

检查：T1 故障注入用例及既有 `store_roundtrip_and_baseline_gate`、`delete_scan_cascades_findings_and_baseline` 通过。

### T5：文档与收尾

- `codescan/DESIGN.md`：历史页差异区补扫描归属、加载/无基线状态和元数据警告。
- `codescan/CHANGELOG.md`：在已发布版本之前新建 `## [Unreleased]`，记录历史页状态一致性、错误反馈和基线事务修复，不改已发布段。
- `codescan/README.md`、`codescan/README.zh-CN.md`：只在功能表的桌面端能力行补「历史页门禁差异显示扫描归属与空/无基线状态」，不改 CLI 的 Baseline & gating 章节。
- Task Spec 全部验收有证据后再将 frontmatter 置为 `status: done`；本计划阶段不提前置 done。

## 3. 验收证据映射

当前均为实施前计划，尚无修复后证据；标注「live 未覆盖」的时序/故障路径以可控自动化测试为最终证据，不宣称真实页面已复现。

Rust 测试前置条件：干净检出先运行 `task build` 生成 `codescan/apps/desktop/dist`。`tauri.conf.json` 的 `frontendDist` 指向该目录且构建产物不入库；没有它，`codescan-desktop` 的 Tauri codegen 无法编译。此条件属于仓库既有约束，不由本次修复引入。

| 验收 | 自动化证据与命令 | 通过判据 | 实面证据 |
|---|---|---|---|
| AC1/AC2 空态与归属 | `pnpm -C codescan/apps/desktop exec vitest run src/App.test.tsx` | 四态均含扫描 id；null 显示无基线 | `pnpm -C codescan dev:app` 走查无基线、空差异、有差异 |
| AC3/AC4 查看与差异竞态 | 同上，deferred 逆序 resolve | 旧响应不能覆盖最后意图；自动差异不压过显式差异 | live 未覆盖精确逆序；真实页只 smoke 最终选择 |
| AC5 错误与部分元数据失败 | 同上，分别 reject IPC | 主操作有 alert；发现保留且元数据 warning 可见 | live 未覆盖 IPC 故障注入 |
| AC6 基线/删除失效 | 同上，先产生差异再变更上下文 | 成功后旧差异消失，在途响应不恢复；失败保留 | 真实页走查切换基线、删除历史 |
| AC7 基线原子性 | `cd codescan && cargo test -p codescan-desktop --test headless_test` | 强制 upsert 失败后行数和活动指纹不变 | 真实 SQLite 故障注入即 live 数据层证据 |
| AC8 integrity | 前端目标测试 | 有值完整展示，缺值明确未记录 | 有规则包历史时走查；无包环境标 live 未覆盖 |
| AC9 回归 | 上述两个目标测试 + `task typecheck` | 全绿，无新增 TS/Rust 错误 | 汇总真实页面截图/观察；未执行项明确保留 |

只有行为、调用方和文档全部完成后才运行上述最终检查各一次；若修复检查暴露的问题，最后编辑后重跑对应目标检查。提交仅在用户另行要求时进行，提交前再执行仓库规定的 `task check`。

## 4. 风险与回滚

- **最后请求判断错误**：重点审查自动差异触发前的 gate 版本比较；AC4 必须覆盖显式请求发生在查看加载期间的情形。
- **失败误清结果**：基线和删除只在成功后失效差异；加载失败只恢复对应请求状态，不清空其他已提交下钻。
- **元数据静默降级**：空数组只能表示成功返回空事件；reject 必须同时产生 warning。
- **事务半写入**：复用既有 helper，故障注入证明回滚；无需手工补偿或删除孤立行迁移。
- **回滚范围**：前端 store、两个历史组件、目标测试和一个 Rust 方法；无 IPC/schema 变化，回滚不会留下数据格式兼容负担。
