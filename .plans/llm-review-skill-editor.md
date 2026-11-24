# Plan：llm-review-skill-editor

对应 Task Spec：`.specs/tasks/llm-review-skill-editor.md`；领域约束：`.specs/domain/scan-engine.md` §3b。

## 契约

```rust
pub const REVIEW_SKILL_FILES: [ReviewSkillFile; 6];
pub struct ReviewSkillOverrides; // name -> body，仅允许内置名称
pub fn review_system_prompt_with(&ReviewSkillOverrides) -> String;
pub fn review_verify_system_prompt_with(&ReviewSkillOverrides) -> String;
```

持久化（`commands.rs`）：

```text
~/.codescan/skills/llm-review.json
{ "version": 1, "files": { "SKILL.md": "...", "references/security.md": "..." } }
```

- 读取：逐项校验名称/空白/单文件与总量上限，非法项跳过；损坏 JSON 回落空覆写。
- 写入：合并后一次写临时文件再 rename；空覆写删除文件。
- 保存内容等于内置正文 → 删除该覆写；命令返回该文件的生效内容与 `customized`。

## 步骤

1. `llm.rs`：提取内置文件表与 base/schema 常量；新增覆写类型、校验与组合提示词；`review`/`review_batched`/`verify_review` 接收覆写。
2. `commands.rs`：覆写读写（临时文件 + rename）、info/save/reset 命令、`llm_review_workspace` 读取覆写。
3. `platform.ts`：技能信息类型与三个 API 包装。
4. `SkillEditorDialog.tsx` + `SettingsView.tsx`：文件表、状态徽标、编辑弹窗、保存/恢复、错误与字节计数。
5. `global.css`：表格列、弹窗宽度、等宽 textarea、计数/超限样式。
6. 测试：`llm.rs` 提示词与校验单测；`headless_test.rs` 临时数据根往返/损坏回落/去覆写；`App.test.tsx` 加载/编辑保存/恢复/空白禁用/失败提示。
7. 文档与真实表面：更新 Domain、`docs/llm.md`、`docs/en/llm.md`、DESIGN、README、CHANGELOG；vite 实拍卡片与弹窗（1280/640）。

## 风险

- **覆写破坏输出**：输出契约与注入边界保持在代码常量中，不进入可编辑文件；空白/超限保存被拒绝。
- **提示词膨胀**：单文件 16 KiB、合计 32 KiB 上限；超限在保存期拒绝，读取期跳过。
- **部分写入**：临时文件 + rename；损坏文件回落内置，不阻断审查。
