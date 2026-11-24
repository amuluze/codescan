/** 内置审查技能编辑器：查看/覆写单个技能文件；保存与恢复由设置页调用后端。 */
import { useEffect, useState } from "react";
import type { ReviewSkillFileInfo } from "../platform";

type Props = {
  file: ReviewSkillFileInfo | null;
  maxBytes: number;
  saving: boolean;
  error: string | null;
  onSave: (content: string) => void;
  onReset: () => void;
  onClose: () => void;
};

export function SkillEditorDialog({
  file,
  maxBytes,
  saving,
  error,
  onSave,
  onReset,
  onClose,
}: Props) {
  const [draft, setDraft] = useState(file?.content ?? "");

  // 文件切换或保存/恢复后内容变化时重填草稿。
  useEffect(() => {
    setDraft(file?.content ?? "");
  }, [file?.name, file?.content]);

  if (!file) return null;

  const bytes = new TextEncoder().encode(draft).length;
  const blank = draft.trim() === "";
  const tooLong = bytes > maxBytes;
  const dirty = draft !== file.content;

  return (
    <div className="modal-backdrop" role="presentation">
      <div
        className="modal skill-editor-modal"
        role="dialog"
        aria-modal="true"
        aria-label={`编辑内置 Skill · ${file.name}`}
      >
        <header className="modal-head">
          <div>
            <h3>编辑内置 Skill</h3>
            <p className="card-sub">
              <span className="mono-cell">{file.name}</span> · {file.title}
            </p>
          </div>
          <button className="ghost" onClick={onClose} aria-label="关闭技能编辑器">
            ✕
          </button>
        </header>

        <p className="key-note">
          编辑只改变模型可见的技能内容；源码不可信声明、JSON 输出校验、复核判定与「不参与门禁」边界固定在代码中，不受覆写影响。
        </p>

        <textarea
          aria-label="技能正文"
          className="skill-editor-textarea"
          onChange={(e) => setDraft(e.target.value)}
          spellCheck={false}
          value={draft}
          wrap="off"
        />

        <div className="skill-editor-meta">
          <span className={tooLong ? "skill-editor-over" : undefined}>
            {bytes} B / 上限 {maxBytes} B
          </span>
          <span>{dirty ? "草稿已修改，尚未保存" : file.customized ? "当前为本地覆写" : "当前为内置默认"}</span>
        </div>

        {error && (
          <p role="alert" className="notice failure skill-editor-error">
            {error}
          </p>
        )}
        {tooLong && (
          <p role="alert" className="notice failure skill-editor-error">
            技能正文超过单文件上限；请精简后再保存。
          </p>
        )}

        <footer className="modal-foot">
          <button
            className="ghost"
            onClick={onReset}
            disabled={!file.customized || saving}
            aria-label={`恢复默认 ${file.name}`}
          >
            恢复默认
          </button>
          <div className="row">
            <button className="ghost" onClick={onClose} disabled={saving}>
              取消
            </button>
            <button
              className="primary"
              onClick={() => onSave(draft)}
              disabled={saving || blank || tooLong}
              aria-label={`保存 ${file.name}`}
            >
              {saving ? "保存中…" : "保存"}
            </button>
          </div>
        </footer>
      </div>
    </div>
  );
}
