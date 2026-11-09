# LLM 辅助层（v3）

Provider 形态复刻 axiom：OpenAI-compatible 与 Anthropic-compatible 两类，BYO API Key，无内置遥测。

## 配置

设置页填写 Provider kind、Base URL、Model 与 API Key：

- API Key 仅写入 `~/.codescan/codescan.db` 的 `secrets` 表（Rust 独占读写）；命令面只暴露「已配置/未配置」，不存在明文回读接口。
- 请求只发往你配置的 Base URL；OpenAI 走 `{base}/chat/completions`（Bearer），Anthropic 走 `{base}/v1/messages`（`x-api-key` + `anthropic-version`）。
- 超时 60s；错误信息不含密钥。

## 两个能力

1. **分诊（参考）**：在发现详情点击「LLM 分诊」，模型返回 `true-positive | likely-false-positive | uncertain` 与理由。结果仅展示，**不写入发现、不参与基线/门禁**（Domain §3a/§5.3）。
2. **候选规则回写**：点击「生成候选规则」，模型产出一条 `custom/` 前缀的规则 YAML；Rust 侧做编译校验（query 语法、枚举、id 唯一、与内置不冲突）。仅当校验通过且你显式点击「保存为自定义规则」后，规则才写入 `~/.codescan/rules/custom.yaml`，并在下一次扫描生效。

## 纪律与边界

- LLM 产出永不进门禁；只有保存后的确定性规则参与扫描与基线。
- 候选规则仅允许 `pattern`（tree-sitter query + `@match`）或 `text`（Rust regex）；不支持 taint/filters（提示词与校验双重限制）。
- 失败可恢复：无密钥、网络错误、模型输出不可解析均返回可读错误，扫描/历史/基线不受影响。
