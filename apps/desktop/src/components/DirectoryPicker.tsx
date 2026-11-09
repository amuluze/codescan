/** 应用内目录选择器：Rust 列目录 + 可编辑路径，不依赖原生对话框。 */
import { useEffect, useState } from "react";
import { api, type DirListing } from "../platform";

type Props = {
  title?: string;
  initialPath?: string | null;
  confirmLabel?: string;
  onSelect: (path: string) => void;
  onClose: () => void;
};

export function DirectoryPicker({
  title = "选择目录",
  initialPath,
  confirmLabel = "选择此目录",
  onSelect,
  onClose,
}: Props) {
  const [listing, setListing] = useState<DirListing | null>(null);
  const [pathInput, setPathInput] = useState(initialPath ?? "");
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);

  const load = async (path?: string) => {
    setLoading(true);
    setError(null);
    try {
      const next = await api.listDirectory(path);
      setListing(next);
      setPathInput(next.path);
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    void load(initialPath ?? undefined);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  return (
    <div className="modal-backdrop" role="dialog" aria-modal="true" aria-label={title}>
      <div className="modal" data-testid="dir-picker">
        <header className="modal-head">
          <h3>{title}</h3>
          <button className="ghost" onClick={onClose} aria-label="关闭">
            ✕
          </button>
        </header>

        <div className="row">
          <input
            aria-label="目录路径"
            className="picker-path"
            value={pathInput}
            onChange={(e) => setPathInput(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") void load(pathInput);
            }}
            placeholder="/path/to/workspace"
          />
          <button onClick={() => void load(pathInput)}>前往</button>
        </div>

        {error && (
          <p role="alert" data-testid="dir-picker-error">
            {error}
          </p>
        )}

        <ul className="dir-list" data-testid="dir-picker-list">
          {listing?.parent && listing.parent !== listing.path && (
            <li>
              <button className="dir-item" onClick={() => void load(listing.parent ?? undefined)}>
                <span className="dir-icon">↰</span> ..
              </button>
            </li>
          )}
          {listing?.entries.map((entry) => (
            <li key={entry.path}>
              <button className="dir-item" onClick={() => void load(entry.path)}>
                <span className="dir-icon">▸</span> {entry.name}
              </button>
            </li>
          ))}
          {listing && listing.entries.length === 0 && (
            <li className="muted empty-dir">此目录没有可进入的子目录</li>
          )}
          {loading && <li className="muted">读取目录…</li>}
        </ul>

        <footer className="modal-foot">
          <span className="muted picker-current" data-testid="dir-picker-current">
            {listing?.path ?? pathInput}
          </span>
          <div className="row">
            <button className="ghost" onClick={onClose}>
              取消
            </button>
            <button
              className="primary"
              data-testid="dir-picker-confirm"
              disabled={!listing}
              onClick={() => listing && onSelect(listing.path)}
            >
              {confirmLabel}
            </button>
          </div>
        </footer>
      </div>
    </div>
  );
}
