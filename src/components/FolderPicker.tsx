import { open } from "@tauri-apps/plugin-dialog";
import { useState } from "react";
import { api } from "../api";

interface Props {
  enabled: boolean;
  currentFolder: string | null;
  onSelected: (path: string) => void;
}

export function FolderPicker({ enabled, currentFolder, onSelected }: Props) {
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  async function handlePick() {
    setError(null);
    const selected = await open({ directory: true, multiple: false, title: "选择文档文件夹" });
    if (!selected || Array.isArray(selected)) return;
    setBusy(true);
    try {
      await api.selectSourceFolder(selected);
      onSelected(selected);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="status-row">
      <span className="status-label">文档文件夹</span>
      <button className="btn" disabled={!enabled || busy} onClick={handlePick}>
        {currentFolder ? "更换文件夹" : "选择文件夹"}
      </button>
      {currentFolder && <span className="folder-path">{currentFolder}</span>}
      {!enabled && <span className="status-text">请先完成上方安装步骤</span>}
      {error && <span className="status-text status-red">{error}</span>}
    </div>
  );
}
