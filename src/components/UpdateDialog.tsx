import { relaunch } from "@tauri-apps/plugin-process";
import { check, type Update } from "@tauri-apps/plugin-updater";
import { useEffect, useRef, useState } from "react";
import { api, onEvent } from "../api";
import { ProgressBar } from "./ProgressBar";
import "./AboutDialog.css";

type Phase =
  | "idle"
  | "checking"
  | "available"
  | "up-to-date"
  | "downloading"
  | "error"
  | "done"
  // Auto mode only: the new version has already been downloaded and
  // installed to disk, but we wait for the user to explicitly confirm
  // before actually restarting into it (see spec item 2).
  | "auto-ready";

const AUTO_CHECK_INTERVAL_MS = 60 * 60 * 1000; // every hour, per spec

export function UpdateDialog() {
  const [visible, setVisible] = useState(false);
  const [phase, setPhase] = useState<Phase>("idle");
  const [update, setUpdate] = useState<Update | null>(null);
  const [percent, setPercent] = useState(0);
  const [error, setError] = useState<string | null>(null);
  const [autoEnabled, setAutoEnabled] = useState(false);
  // Guards against overlapping runs - e.g. the user clicks "检查更新"
  // manually right as an hourly auto-check happens to fire.
  const busyRef = useRef(false);

  useEffect(() => {
    const unlistenPromise = onEvent<void>("open-update-check", () => {
      setVisible(true);
      runCheck();
    });
    return () => {
      unlistenPromise.then((u) => u());
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // Initial value from the persisted config, then live updates whenever the
  // "自动检查更新" switch in AboutDialog is flipped (so the hourly timer
  // below starts/stops immediately, no app restart needed).
  useEffect(() => {
    api.getConfig().then((cfg) => setAutoEnabled(cfg.auto_update_enabled));
    const unlistenPromise = onEvent<boolean>("auto-update-setting-changed", setAutoEnabled);
    return () => {
      unlistenPromise.then((u) => u());
    };
  }, []);

  useEffect(() => {
    if (!autoEnabled) return;

    async function autoTick() {
      if (busyRef.current) return;
      busyRef.current = true;
      try {
        const result = await check();
        if (!result) return; // already up to date - stay completely silent
        setUpdate(result);
        setVisible(true);
        setPhase("downloading");
        setPercent(0);
        let downloaded = 0;
        let total = 0;
        await result.downloadAndInstall((event) => {
          if (event.event === "Started") {
            total = event.data.contentLength ?? 0;
          } else if (event.event === "Progress") {
            downloaded += event.data.chunkLength;
            setPercent(total > 0 ? (downloaded / total) * 100 : 0);
          } else if (event.event === "Finished") {
            setPercent(100);
          }
        });
        // Installed on disk now; per spec we still wait for the user to
        // confirm before actually restarting into it.
        setPhase("auto-ready");
      } catch (e) {
        // A background check the user didn't ask for shouldn't interrupt
        // them with an error dialog - log it and just try again next hour.
        console.error("自动检查更新失败:", e);
      } finally {
        busyRef.current = false;
      }
    }

    autoTick(); // also check once immediately when auto mode turns on
    const id = setInterval(autoTick, AUTO_CHECK_INTERVAL_MS);
    return () => clearInterval(id);
  }, [autoEnabled]);

  async function runCheck() {
    if (busyRef.current) return;
    busyRef.current = true;
    setPhase("checking");
    setError(null);
    try {
      const result = await check();
      if (result) {
        setUpdate(result);
        setPhase("available");
      } else {
        setPhase("up-to-date");
      }
    } catch (e) {
      setError(String(e));
      setPhase("error");
    } finally {
      busyRef.current = false;
    }
  }

  async function handleInstall() {
    if (!update) return;
    setPhase("downloading");
    setPercent(0);
    let downloaded = 0;
    let total = 0;
    try {
      await update.downloadAndInstall((event) => {
        if (event.event === "Started") {
          total = event.data.contentLength ?? 0;
        } else if (event.event === "Progress") {
          downloaded += event.data.chunkLength;
          setPercent(total > 0 ? (downloaded / total) * 100 : 0);
        } else if (event.event === "Finished") {
          setPercent(100);
        }
      });
      setPhase("done");
      await relaunch();
    } catch (e) {
      setError(String(e));
      setPhase("error");
    }
  }

  if (!visible) return null;

  const canClose = phase !== "downloading";

  return (
    <div className="about-overlay" onClick={() => canClose && setVisible(false)}>
      <div className="about-dialog" onClick={(e) => e.stopPropagation()}>
        <h2>检查更新</h2>

        {phase === "checking" && <p>正在检查新版本...</p>}
        {phase === "up-to-date" && <p>当前已是最新版本。</p>}
        {phase === "available" && update && (
          <>
            <p>
              发现新版本 {update.version}
              {update.body ? `：${update.body}` : ""}
            </p>
            <button className="btn" onClick={handleInstall}>
              下载并安装
            </button>
          </>
        )}
        {phase === "downloading" && (
          <>
            <p>正在下载更新{update ? ` ${update.version}` : ""}...</p>
            <ProgressBar percent={percent} />
          </>
        )}
        {phase === "done" && <p>安装完成，正在重启应用...</p>}
        {phase === "auto-ready" && update && (
          <>
            <p>新版本 {update.version} 已自动下载并安装完成，重启应用后生效。</p>
            <button className="btn" onClick={() => relaunch()}>
              现在重启
            </button>
          </>
        )}
        {phase === "error" && <p className="status-red">检查更新失败：{error}</p>}

        <button className="btn btn-secondary" onClick={() => setVisible(false)} disabled={!canClose}>
          {phase === "auto-ready" ? "稍后重启" : "关闭"}
        </button>
      </div>
    </div>
  );
}
