interface Props {
  percent: number;
  message?: string;
}

export function ProgressBar({ percent, message }: Props) {
  const clamped = Math.max(0, Math.min(100, percent));
  return (
    <div className="progress-wrap">
      <div className="progress-bar-track">
        <div className="progress-bar-fill" style={{ width: `${clamped}%` }} />
      </div>
      <span className="progress-percent">{clamped.toFixed(0)}%</span>
      {message ? <span className="progress-message">{message}</span> : null}
    </div>
  );
}
