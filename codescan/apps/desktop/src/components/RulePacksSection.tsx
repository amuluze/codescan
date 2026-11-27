/** 规则包管理区：安装入口、签名状态与卸载。 */
import type { RulePackInfo } from "../platform";

type Props = {
  packs: RulePackInfo[];
  error: string | null;
  busy: boolean;
  onInstall: () => void;
  onRemove: (pack: RulePackInfo) => void;
};

export function RulePacksSection({ packs, error, busy, onInstall, onRemove }: Props) {
  return (
    <section className="detail-section" data-testid="rule-packs">
      <div className="row">
        <h3>规则包</h3>
        <button className="ghost btn-sm" onClick={onInstall} disabled={busy}>
          安装规则包
        </button>
      </div>
      {error && (
        <p role="alert" data-testid="rule-pack-error">
          {error}
        </p>
      )}
      {packs.length === 0 ? (
        <p className="muted">尚未安装规则包。规则包目录需包含 pack.yaml。</p>
      ) : (
        <table className="rules-table">
          <thead>
            <tr>
              <th>id</th>
              <th>版本</th>
              <th>规则数</th>
              <th>状态</th>
              <th>操作</th>
            </tr>
          </thead>
          <tbody>
            {packs.map((pack) => (
              <tr key={`${pack.id}@${pack.version}`}>
                <td>{pack.id}</td>
                <td>{pack.version}</td>
                <td>{pack.rules}</td>
                <td>
                  {pack.error ? (
                    <span className="degraded" title={pack.error}>
                      加载失败
                    </span>
                  ) : pack.signed_by ? (
                    <span title={pack.signed_by}>已安装 · 已签名</span>
                  ) : (
                    "已安装"
                  )}
                </td>
                <td>
                  <button
                    className="ghost btn-sm"
                    disabled={busy}
                    onClick={() => onRemove(pack)}
                  >
                    卸载
                  </button>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </section>
  );
}
