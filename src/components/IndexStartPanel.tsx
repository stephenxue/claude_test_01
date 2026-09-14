import { ProgressBar } from "./ProgressBar";
import type { IndexProgressEvent } from "../types";
import "./IndexStartPanel.css";

interface Props {
  folder: string;
  indexing: boolean;
  indexProgress: IndexProgressEvent | null;
  onStart: () => void;
}

function formatEta(seconds: number | null | undefined): string {
  if (seconds == null) return "";
  if (seconds < 1) return "即将完成";
  if (seconds < 60) return `预计剩余约 ${Math.ceil(seconds)} 秒`;
  return `预计剩余约 ${Math.ceil(seconds / 60)} 分钟`;
}

export function IndexStartPanel({ folder, indexing, indexProgress, onStart }: Props) {
  return (
    <div className="index-start-panel">
      <h2>文件夹已选择</h2>
      <p className="index-start-folder">{folder}</p>
      <p className="index-start-hint">
        点击下方按钮开始扫描并向量化这个文件夹里的文本文件。之后新增或删除文件后，
        可以在下方文档列表里随时点“重新扫描文件夹”再次同步。
      </p>

      {!indexing ? (
        <button className="btn btn-primary-lg" onClick={onStart}>
          开始向量化
        </button>
      ) : (
        <div className="index-start-progress">
          <p>
            {indexProgress?.stage === "ingesting" && `正在向量化: ${indexProgress.file_name}`}
            {(!indexProgress || indexProgress.stage === "done") && "正在扫描文件夹..."}
          </p>
          {indexProgress?.stage === "ingesting" && indexProgress.eta_seconds != null && (
            <p className="index-start-eta">{formatEta(indexProgress.eta_seconds)}</p>
          )}
          <ProgressBar
            percent={
              indexProgress && indexProgress.total > 0
                ? (indexProgress.current / indexProgress.total) * 100
                : 0
            }
          />
        </div>
      )}
    </div>
  );
}
