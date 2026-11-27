/**
 * 规则页的构建块：页面态 hook（规则包）与各区段展示组件。
 * 展示组件均由 props 驱动、无自有状态；页面骨架与 store 接线见 RulesView.tsx。
 */
import { useState, type Dispatch, type SetStateAction } from "react";
import { api, type RuleInfo, type RulePackInfo } from "../platform";

/** 规则包的页面态：列表、安装/卸载与忙碌态。 */
type RulePacksModel = {
  packs: RulePackInfo[];
  packError: string | null;
  packBusy: boolean;
  packPickerOpen: boolean;
  setPackPickerOpen: Dispatch<SetStateAction<boolean>>;
  loadPacks: () => Promise<void>;
  installPack: (sourceDir: string) => Promise<void>;
  removePack: (pack: RulePackInfo) => Promise<void>;
};

/** 规则包：列表、安装/卸载与忙碌态；变更后连同规则目录一起刷新。 */
export function useRulePacks(load: () => Promise<void>): RulePacksModel {
  const [packs, setPacks] = useState<RulePackInfo[]>([]);
  const [packError, setPackError] = useState<string | null>(null);
  const [packPickerOpen, setPackPickerOpen] = useState(false);
  const [packBusy, setPackBusy] = useState(false);

  const loadPacks = async () => {
    try {
      setPacks(await api.listRulePacks());
      setPackError(null);
    } catch (error) {
      setPackError(String(error));
    }
  };

  const installPack = async (sourceDir: string) => {
    setPackPickerOpen(false);
    setPackBusy(true);
    setPackError(null);
    try {
      await api.installRulePack(sourceDir);
      await load();
      await loadPacks();
    } catch (error) {
      setPackError(String(error));
    } finally {
      setPackBusy(false);
    }
  };

  const removePack = async (pack: RulePackInfo) => {
    setPackBusy(true);
    setPackError(null);
    try {
      await api.removeRulePack(pack.id, pack.version);
      await load();
      await loadPacks();
    } catch (error) {
      setPackError(String(error));
    } finally {
      setPackBusy(false);
    }
  };

  return {
    packs,
    packError,
    packBusy,
    packPickerOpen,
    setPackPickerOpen,
    loadPacks,
    installPack,
    removePack,
  };
}

/** 规则目录表：启停复选框（不冒泡到行点击）+ 只读字段 + 编译错误标注。 */
export function RulesTable({
  rules,
  selectedId,
  onOpen,
  onToggle,
}: {
  rules: RuleInfo[];
  selectedId: string | null;
  onOpen: (rule: RuleInfo) => void;
  onToggle: (ruleId: string, enabled: boolean) => void;
}) {
  return (
    <table className="rules-table">
      <thead>
        <tr>
          <th>启用</th>
          <th>id</th>
          <th>语言</th>
          <th>维度</th>
          <th>严重级</th>
          <th>来源</th>
          <th>说明</th>
        </tr>
      </thead>
      <tbody data-testid="rules-table">
        {rules.map((r) => (
          <tr
            key={r.id}
            className={`${r.error ? "rule-error-row" : ""} ${
              selectedId === r.id ? "selected" : ""
            }`}
            onClick={() => void onOpen(r)}
          >
            <td>
              <input
                type="checkbox"
                aria-label={`启用 ${r.id}`}
                checked={r.enabled}
                disabled={Boolean(r.error)}
                onClick={(event) => event.stopPropagation()}
                onChange={(e) => void onToggle(r.id, e.target.checked)}
              />
            </td>
            <td>{r.id}</td>
            <td>{r.language}</td>
            <td>{r.dimension}</td>
            <td>{r.severity}</td>
            <td>{r.origin ?? ""}</td>
            <td>
              {r.error ? (
                <span className="degraded" title={r.error}>
                  {r.message}
                </span>
              ) : (
                r.message
              )}
            </td>
          </tr>
        ))}
      </tbody>
    </table>
  );
}

/** 规则包区：安装入口 + 已安装包（签名 / 加载失败状态）与卸载。 */
export function RulePacksSection({ model }: { model: RulePacksModel }) {
  const { packs, packError, packBusy, setPackPickerOpen, removePack } = model;
  return (
    <section className="detail-section" data-testid="rule-packs">
      <div className="row">
        <h3>规则包</h3>
        <button
          className="ghost btn-sm"
          onClick={() => setPackPickerOpen(true)}
          disabled={packBusy}
        >
          安装规则包
        </button>
      </div>
      {packError && (
        <p role="alert" data-testid="rule-pack-error">
          {packError}
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
                    disabled={packBusy}
                    onClick={() => void removePack(pack)}
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

/** 内置（只读）规则详情。 */
export function RuleReadOnlyDetail({ rule }: { rule: RuleInfo }) {
  return (
    <section className="detail-section" data-testid="rule-detail">
      <h3>{rule.id}</h3>
      <p className="muted">
        {rule.language} · {rule.dimension} · {rule.severity}
        {rule.error ? " · 规则错误" : ""}
      </p>
      <pre className="code-block">
        <code>{rule.source}</code>
      </pre>
    </section>
  );
}
