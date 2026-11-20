/**
 * 应用更新状态：后台定期检查与设置页手动检查共享同一份事实。
 *
 * 更新对象（downloadAndInstall 所需的句柄）保留在平台层（platform.ts）；
 * 本 store 只承载展示层事实：是否发现新版本、检查/安装进度、提示与错误。
 */
import { create } from "zustand";
import { updater, type UpdateInfo } from "../platform";

type UpdateState = {
  /** 已发现的新版本；null 表示当前无可用更新。 */
  available: UpdateInfo | null;
  /** 手动检查进行中（后台静默检查不置位，避免打扰设置页按钮态）。 */
  checking: boolean;
  installing: boolean;
  /** 手动检查的状态提示（如「已是最新版本」）。 */
  message: string | null;
  error: string | null;
  /** 手动检查更新：有更新写入 available，无更新提示「已是最新版本」，失败给出可读错误。 */
  check: () => Promise<void>;
  /** 后台静默检查：失败不打扰用户（无 updater 配置的构建访问端点必然失败）。 */
  checkSilently: () => Promise<void>;
  /** 下载安装已发现的更新，随后重启应用。 */
  install: () => Promise<void>;
};

/** 在途检查：后台定时检查与设置页手动检查共用同一次请求，避免重复打端点。 */
let inflight: Promise<UpdateInfo | null> | null = null;

function requestCheck(): Promise<UpdateInfo | null> {
  if (!inflight) {
    inflight = updater.check().finally(() => {
      inflight = null;
    });
  }
  return inflight;
}

export const useUpdateStore = create<UpdateState>((set, get) => ({
  available: null,
  checking: false,
  installing: false,
  message: null,
  error: null,

  async check() {
    set({ checking: true, message: null, error: null });
    try {
      const info = await requestCheck();
      if (info) {
        set({ available: info, checking: false });
      } else {
        set({ available: null, checking: false, message: "已是最新版本" });
      }
    } catch (e) {
      set({ checking: false, error: String(e) });
    }
  },

  async checkSilently() {
    try {
      const info = await requestCheck();
      set({ available: info });
    } catch {
      // 后台检查失败静默：无 updater 配置的构建与网络不可达都不该打扰用户。
    }
  },

  async install() {
    if (!get().available) {
      set({ error: "没有待安装的更新（请先检查更新）" });
      return;
    }
    set({ installing: true, error: null });
    try {
      await updater.installAndRelaunch();
      set({ installing: false, message: "更新已安装，请重启应用完成升级" });
    } catch (e) {
      set({ installing: false, error: String(e) });
    }
  },
}));
