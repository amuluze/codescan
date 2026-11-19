/** 一次性操作的结果提示对话框（导出完成/失败）；复用通用 modal 样式。 */
type Props = {
  tone: "success" | "error";
  title: string;
  message: string;
  onClose: () => void;
};

export function NoticeDialog({ tone, title, message, onClose }: Props) {
  return (
    <div className="modal-backdrop" role="presentation">
      <div className="modal notice-modal" role="dialog" aria-modal="true" aria-label={title}>
        <header className="modal-head">
          <h3>{title}</h3>
          <button className="ghost" onClick={onClose} aria-label="关闭">
            ✕
          </button>
        </header>
        <p
          className={`notice-message ${tone === "error" ? "run-error review-error" : ""}`}
          data-testid="notice-message"
        >
          {message}
        </p>
        <footer className="modal-foot">
          <div className="row">
            <button className="primary" data-testid="notice-dismiss" onClick={onClose}>
              知道了
            </button>
          </div>
        </footer>
      </div>
    </div>
  );
}
