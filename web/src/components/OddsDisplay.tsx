import type { OddsResult } from '../types/odds';
import { HAND_CATEGORIES } from '../types/odds';

interface Props {
  result: OddsResult | null;
  calculating: boolean;
}

function pct(v: number) {
  return (v * 100).toFixed(1);
}

interface BarProps {
  label: string;
  value: number;
  colorClass: string;
}

function OddsBar({ label, value, colorClass }: BarProps) {
  const p = value * 100;
  return (
    <div className="odds-bar-row">
      <span className="odds-bar-label">{label}</span>
      <div className="odds-bar-track">
        <div
          className={`odds-bar-fill ${colorClass}`}
          style={{ width: `${p.toFixed(2)}%` }}
        />
      </div>
      <span className="odds-bar-pct">{pct(value)}%</span>
    </div>
  );
}

export default function OddsDisplay({ result, calculating }: Props) {
  if (calculating && !result) {
    return (
      <div className="odds-loading">
        <div className="loading-ring" />
        <p>Running simulation…</p>
      </div>
    );
  }

  if (!result) return null;

  return (
    <div className="odds-display">
      {/* Win / Tie / Lose bars */}
      <div className="odds-bars">
        <OddsBar label="Win"  value={result.win}  colorClass="win"  />
        <OddsBar label="Tie"  value={result.tie}  colorClass="tie"  />
        <OddsBar label="Lose" value={result.lose} colorClass="lose" />
      </div>

      {/* Big numbers */}
      <div className="odds-numbers">
        <div className="odds-number win">
          <span className="odds-big">{pct(result.win)}%</span>
          <span className="odds-sub">Win</span>
        </div>
        <div className="odds-number tie">
          <span className="odds-big">{pct(result.tie)}%</span>
          <span className="odds-sub">Tie</span>
        </div>
        <div className="odds-number lose">
          <span className="odds-big">{pct(result.lose)}%</span>
          <span className="odds-sub">Lose</span>
        </div>
      </div>

      {/* Hand distribution */}
      <div className="hand-dist">
        <h3 className="hand-dist-title">Hand Distribution</h3>
        <div className="hand-dist-grid">
          {[...HAND_CATEGORIES].reverse().map((cat) => {
            const v = result.hand_distribution[cat] ?? 0;
            const p = v * 100;
            if (p < 0.005) return null; // hide near-zero entries
            return (
              <div key={cat} className="hand-dist-row">
                <span className="hand-dist-name">{cat}</span>
                <div className="hand-dist-track">
                  <div
                    className="hand-dist-fill"
                    style={{ width: `${Math.min(p, 100).toFixed(2)}%` }}
                  />
                </div>
                <span className="hand-dist-pct">{p.toFixed(1)}%</span>
              </div>
            );
          })}
        </div>
      </div>

      {/* Sim metadata */}
      <div className="sim-meta">
        <span className="sim-badge">{result.method}</span>
        <span className="sim-sims">{result.simulations_run.toLocaleString()} simulations</span>
      </div>
    </div>
  );
}
