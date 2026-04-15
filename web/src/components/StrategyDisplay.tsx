import type { SolverResult } from '../types/solver';

interface Props {
  result: SolverResult;
}

export default function StrategyDisplay({ result }: Props) {
  return (
    <div className="strategy-display">
      {/* Summary stats */}
      <div className="strategy-stats">
        <div className="stat">
          <span className="stat-label">Game Value (P0)</span>
          <span className={`stat-value ${result.game_value >= 0 ? 'win' : 'lose'}`}>
            {result.game_value >= 0 ? '+' : ''}{result.game_value.toFixed(4)}
          </span>
        </div>
        <div className="stat">
          <span className="stat-label">Exploitability</span>
          <span className={`stat-value ${result.exploitability_mbb < 10 ? 'win' : result.exploitability_mbb < 50 ? 'tie' : 'lose'}`}>
            {result.exploitability_mbb.toFixed(2)} mbb/hand
          </span>
        </div>
        <div className="stat">
          <span className="stat-label">Info Sets</span>
          <span className="stat-value">{result.num_info_sets.toLocaleString()}</span>
        </div>
        <div className="stat">
          <span className="stat-label">Nodes</span>
          <span className="stat-value">{result.num_nodes.toLocaleString()}</span>
        </div>
      </div>

      {/* Strategy table */}
      <h3 className="section-title">Strategy</h3>
      <div className="strategy-table">
        {result.strategies.length === 0 ? (
          <p className="strategy-empty">No strategies to display</p>
        ) : (
          result.strategies.map((entry, i) => (
            <div key={i} className="strategy-row">
              <span className="strategy-label">{entry.label}</span>
              <div className="strategy-actions">
                {entry.actions.map((action, j) => {
                  const pct = action.probability * 100;
                  const colorClass =
                    pct > 60 ? 'bar-high' : pct > 20 ? 'bar-mid' : 'bar-low';
                  return (
                    <div key={j} className="action-bar-container">
                      <span className="action-name">{action.name}</span>
                      <div className="action-bar-track">
                        <div
                          className={`action-bar-fill ${colorClass}`}
                          style={{ width: `${pct}%` }}
                        />
                      </div>
                      <span className="action-pct">{pct.toFixed(1)}%</span>
                    </div>
                  );
                })}
              </div>
            </div>
          ))
        )}
      </div>
    </div>
  );
}
