# codescan CLI

`codescan` 是 Codescan 确定性扫描引擎的无头入口，用于本地检查与 CI 门禁。扫描结果与桌面应用同源：同一引擎版本、同一规则集、同一指纹与基线语义。

## 安装 / 运行

```bash
cargo run -p codescan-cli -- scan <root>
# 或构建后使用 target/debug/codescan
```

## 命令

```text
codescan scan <root> [OPTIONS]
```

| 选项 | 说明 |
| --- | --- |
| `--format text\|json\|sarif` | 输出格式，默认 `text`。`json` 为 `ScanReport`，`sarif` 为 SARIF 2.1.0。 |
| `--out <path>` | 将所选格式写入文件；此时 stdout 只打印一行汇总 `files=X findings=Y events=Z`。未给出时写到 stdout（text 首行即汇总）。 |
| `--rules <path>` | 自定义规则 YAML，叠加在内置规则之上；文件不存在或编译失败 → 退出码 2。 |
| `--global-ignore <path>` | 额外全局忽略文件（gitignore 语法），与 `.gitignore`、`.codescanignore` 及内置默认排除目录共同生效。 |
| `--baseline <path>` | 读取基线 JSON（`Baseline`）；文件不存在或解析失败 → 退出码 2。 |
| `--save-baseline <path>` | 将当前发现的指纹快照以 pretty JSON 写入文件。 |
| `--fail-on new\|error\|warning\|none` | 门禁条件，默认 `none`。`new` 必须同时提供 `--baseline`，否则退出码 2。 |

### text 输出

```text
files=12 findings=2 events=0
src/a.ts:1:1 text/todo-debt info TODO/FIXME 债务标记
```

每行发现格式为 `path:line:col rule_id severity message`，路径为工作区相对路径，顺序由引擎确定性排序保证。

## 退出码

| 码 | 含义 |
| --- | --- |
| `0` | 扫描完成且门禁通过（或 `--fail-on none`）。 |
| `1` | 门禁命中：`error` / `warning` 级发现存在，或 `new` 模式下存在基线外发现。 |
| `2` | 用法或运行时错误（参数非法、规则/基线文件缺失或解析失败、扫描 IO 错误等），错误信息写入 stderr。 |

## CI 示例

首次建立基线并提交（例如 `baseline.json`）：

```bash
codescan scan . --save-baseline baseline.json
```

后续流水线只阻止新增问题：

```bash
codescan scan . \
  --baseline baseline.json \
  --fail-on new \
  --format sarif \
  --out codescan.sarif
```

- 退出码 `0`：没有基线外的新发现。
- 退出码 `1`：出现新发现，流水线失败；`codescan.sarif` 可供代码扫描平台展示。

若要临时收紧为全量门禁，可改用 `--fail-on error` 或 `--fail-on warning`；它们不依赖基线。
