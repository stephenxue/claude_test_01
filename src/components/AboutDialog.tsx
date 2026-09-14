import { open } from "@tauri-apps/plugin-dialog";
import { open as openUrl } from "@tauri-apps/plugin-shell";
import { useEffect, useState } from "react";
import { api, emitEvent, onEvent } from "../api";
import "./AboutDialog.css";

// TODO: replace with your real company name and website before shipping.
const COMPANY_NAME = "示例科技有限公司 (Example Tech Co., Ltd.)";
const COMPANY_URL = "https://example.com";
const APP_VERSION = "0.1.0";

type ImportStatus = "idle" | "working" | "error";

export function AboutDialog() {
  const [visible, setVisible] = useState(false);
  const [autoUpdate, setAutoUpdate] = useState(false);
  const [importStatus, setImportStatus] = useState<ImportStatus>("idle");
  const [importError, setImportError] = useState<string | null>(null);

  useEffect(() => {
    const unlistenPromise = onEvent<void>("open-about", () => setVisible(true));
    return () => {
      unlistenPromise.then((u) => u());
    };
  }, []);

  useEffect(() => {
    api.getConfig().then((cfg) => setAutoUpdate(cfg.auto_update_enabled));
  }, [visible]);

  async function handleToggleAutoUpdate() {
    const next = !autoUpdate;
    setAutoUpdate(next);
    try {
      await api.setAutoUpdateEnabled(next);
      // Tell UpdateDialog's hourly timer to start/stop right away, without
      // requiring an app restart.
      await emitEvent("auto-update-setting-changed", next);
    } catch (e) {
      setAutoUpdate(!next); // revert on failure
      console.error("保存自动更新设置失败:", e);
    }
  }

  function handleCheckNow() {
    // Reuses the same trigger the menu bar's "检查更新" item uses -
    // UpdateDialog listens for this and opens/runs the check itself.
    emitEvent("open-update-check", undefined);
  }

  async function handleImportUpdate() {
    setImportError(null);
    const path = await open({
      multiple: false,
      title: "选择更新包 (.dmg 或 .app.tar.gz)",
      filters: [{ name: "更新包", extensions: ["dmg", "gz", "tgz"] }],
    });
    if (!path || Array.isArray(path)) return;
    setImportStatus("working");
    try {
      // On success this never actually returns - the app relaunches into
      // the new version - so we don't need a "done" state to handle.
      await api.installUpdateFromFile(path);
    } catch (e) {
      setImportStatus("error");
      setImportError(String(e));
    }
  }

  if (!visible) return null;

  return (
    <div className="about-overlay" onClick={() => setVisible(false)}>
      <div className="about-dialog" onClick={(e) => e.stopPropagation()}>
        <h2>本地知识库助手</h2>
        <p className="about-version">版本 {APP_VERSION}</p>
        <p>{COMPANY_NAME}</p>
        <p>
          <a href="#" onClick={(e) => { e.preventDefault(); openUrl(COMPANY_URL); }}>
            {COMPANY_URL}
          </a>
        </p>
        <p className="about-credits">
          本应用使用 Ollama 及开源模型（Qwen2.5 / Llama 3 / BAAI bge-m3）在本地运行，
          所有文档与对话数据均保存在本机，不会上传至云端。
        </p>

        <div className="about-update-section">
          <label className="about-toggle-row">
            <span>自动检查更新（每小时，发现新版本会自动下载安装，重启前会先询问）</span>
            <input type="checkbox" checked={autoUpdate} onChange={handleToggleAutoUpdate} />
          </label>

          <div className="about-update-actions">
            <button className="btn btn-secondary" onClick={handleCheckNow}>
              立即检查更新
            </button>
            <button className="btn btn-secondary" onClick={handleImportUpdate} disabled={importStatus === "working"}>
              {importStatus === "working" ? "正在导入..." : "导入更新包..."}
            </button>
          </div>
          <p className="about-update-hint">
            无法访问 GitHub（例如身处中国大陆）时，可以让能访问的朋友把发布包（.dmg 或
            .app.tar.gz）传给你，再用"导入更新包"手动安装，完全不需要联网。
          </p>
          {importStatus === "error" && <p className="status-red">导入失败：{importError}</p>}
        </div>

        <button className="btn" onClick={() => setVisible(false)}>
          关闭
        </button>
      </div>
    </div>
  );
}
