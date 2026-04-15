import type { SolverProgress } from '../types/solver';

interface Props {
  progress: SolverProgress | null;
  onCancel: () => void;
  solving: boolean;
}

export default function ProgressBar({ progress, onCancel, solving }: Props) {
  if (!progress && !solving) return null;

  const pct = progress
    ? Math.round((progress.iteration / progress.total) * 100)
    : 0;

  return (
    <div className="solver-progress">
      <div className="progress-header">
        <span className="progress-label">
          {solving ? 'Solving...' : 'Complete'}
        </span>
        {progress && (
          <span className="progress-stats">
            {progress.iteration.toLocaleString()} / {progress.total.toLocaleString()} iterations
          </span>
        )}
        {solving && (
          <button className="btn-cancel" onClick={onCancel}>Cancel</button>
        )}
      </div>
      <div className="progress-bar-track">
        <div
          className="progress-bar-fill"
          style={{ width: `${pct}%` }}
        />
      </div>
      {progress && (
        <div className="progress-footer">
          <span>Game Value: {progress.game_value >= 0 ? '+' : ''}{progress.game_value.toFixed(4)}</span>
          <span>{pct}%</span>
        </div>
      )}
    </div>
  );
}
