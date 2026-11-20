/**
 * 定期检查应用更新：应用启动即查一次，之后按固定间隔复查。
 *
 * 后台检查静默失败（无 updater 配置的构建与网络不可达都不打扰用户），
 * 发现新版本时只更新 store 的 available，由顶栏升级图标提示。
 */
import { useEffect } from "react";
import { useUpdateStore } from "./stores/updateStore";

/** 复查间隔：6 小时。 */
export const UPDATE_CHECK_INTERVAL_MS = 6 * 60 * 60 * 1000;

export function useUpdateCheck(): void {
  useEffect(() => {
    const run = () => {
      void useUpdateStore.getState().checkSilently();
    };
    run();
    const timer = setInterval(run, UPDATE_CHECK_INTERVAL_MS);
    return () => clearInterval(timer);
  }, []);
}
