/** 展示层导出动作：导出中状态 + 完成/失败对话框。 */
import { useState } from "react";
import { api } from "./platform";

export type ExportNotice = {
  tone: "success" | "error";
  title: string;
  message: string;
};

/**
 * 统一导出行为：调用 `export_report`，期间 `exporting` 为真，
 * 结束后以 `notice` 返回完成或失败信息（由 NoticeDialog 呈现）。
 */
export function useReportExport() {
  const [exporting, setExporting] = useState(false);
  const [notice, setNotice] = useState<ExportNotice | null>(null);

  const run = async (dest: string, base: string, markdown: string, json: string) => {
    setExporting(true);
    setNotice(null);
    try {
      await api.exportReport(dest, base, markdown, json);
      setNotice({
        tone: "success",
        title: "导出完成",
        message: `已写入 2 个文件：\n${base}.md\n${base}.json\n目录：${dest}`,
      });
    } catch (e) {
      setNotice({ tone: "error", title: "导出失败", message: String(e) });
    } finally {
      setExporting(false);
    }
  };

  return { exporting, notice, run, dismiss: () => setNotice(null) };
}
