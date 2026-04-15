import { useCallback, useEffect, useMemo, useState } from 'react';
import type { Backend } from '../api/backend';
import type { SolverConfigDto, SolverProgress, SolverResult } from '../types/solver';
import { defaultSolverConfig, streetBoardCount, type Street } from '../types/solver';
import CardGrid from '../components/CardGrid';
import CardSlots from '../components/CardSlots';
import RangeInput from '../components/RangeInput';
import ProgressBar from '../components/ProgressBar';
import StrategyDisplay from '../components/StrategyDisplay';

interface Props {
  backend: Backend;
}

export default function GtoSolver({ backend }: Props) {
  const [street, setStreet] = useState<Street>('river');
  const [boardCards, setBoardCards] = useState<string[]>([]);
  const [config, setConfig] = useState<SolverConfigDto>(defaultSolverConfig());

  const [solving, setSolving] = useState(false);
  const [solveId, setSolveId] = useState<string | null>(null);
  const [progress, setProgress] = useState<SolverProgress | null>(null);
  const [result, setResult] = useState<SolverResult | null>(null);
  const [error, setError] = useState<string | null>(null);

  const boardCount = streetBoardCount(street);
  const usedCards = useMemo(() => new Set(boardCards), [boardCards]);

  // Subscribe to solver events
  useEffect(() => {
    const unProg = backend.onSolverProgress((p) => {
      if (solveId && p.solve_id === solveId) {
        setProgress(p);
      }
    });
    const unComplete = backend.onSolverComplete((r) => {
      if (solveId && r.solve_id === solveId) {
        setResult(r);
        setSolving(false);
      }
    });
    return () => {
      unProg();
      unComplete();
    };
  }, [backend, solveId]);

  const handleBoardCardClick = useCallback(
    (cardId: string) => {
      if (boardCards.includes(cardId)) {
        setBoardCards((prev) => prev.filter((c) => c !== cardId));
      } else if (boardCards.length < boardCount) {
        setBoardCards((prev) => [...prev, cardId]);
      }
    },
    [boardCards, boardCount],
  );

  const handleStreetChange = useCallback((s: Street) => {
    setStreet(s);
    setBoardCards([]);
    setResult(null);
    setProgress(null);
  }, []);

  const canSolve = boardCards.length === boardCount && !solving;

  const handleSolve = useCallback(async () => {
    if (!canSolve) return;
    setError(null);
    setResult(null);
    setProgress(null);
    setSolving(true);

    const solverConfig: SolverConfigDto = {
      ...config,
      board: boardCards,
    };

    try {
      const id = await backend.startSolve(solverConfig);
      setSolveId(id);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
      setSolving(false);
    }
  }, [canSolve, config, boardCards, backend]);

  const handleCancel = useCallback(async () => {
    if (solveId) {
      try {
        await backend.cancelSolve(solveId);
      } catch {
        // ignore
      }
      setSolving(false);
    }
  }, [backend, solveId]);

  const handleReset = useCallback(() => {
    setBoardCards([]);
    setResult(null);
    setProgress(null);
    setError(null);
    setSolving(false);
    setConfig(defaultSolverConfig());
  }, []);

  return (
    <main className="main">
      {/* Left column: configuration */}
      <div className="setup-column">
        {/* Street selector */}
        <section className="card-section">
          <h2 className="section-title">Street</h2>
          <div className="street-selector">
            {(['flop', 'turn', 'river'] as Street[]).map((s) => (
              <button
                key={s}
                className={`variant-btn ${street === s ? 'active' : ''}`}
                onClick={() => handleStreetChange(s)}
              >
                {s.charAt(0).toUpperCase() + s.slice(1)}
                <span className="variant-desc">{streetBoardCount(s)} cards</span>
              </button>
            ))}
          </div>
        </section>

        {/* Board cards */}
        <section className="card-section">
          <h2 className="section-title">
            Board Cards
            <span className="hint"> ({boardCards.length}/{boardCount})</span>
          </h2>
          <CardSlots
            label="Board"
            cards={boardCards}
            count={boardCount}
            active={true}
            onClick={() => {}}
            onRemove={(c) => setBoardCards((prev) => prev.filter((x) => x !== c))}
          />
        </section>

        {/* Ranges */}
        <section className="card-section">
          <h2 className="section-title">Player Ranges</h2>
          <RangeInput
            label="OOP (Out of Position)"
            value={config.range_oop}
            onChange={(v) => setConfig((c) => ({ ...c, range_oop: v }))}
            backend={backend}
          />
          <RangeInput
            label="IP (In Position)"
            value={config.range_ip}
            onChange={(v) => setConfig((c) => ({ ...c, range_ip: v }))}
            backend={backend}
          />
        </section>

        {/* Solver config */}
        <section className="card-section">
          <h2 className="section-title">Configuration</h2>

          <div className="config-grid">
            <label className="config-label">Algorithm</label>
            <div className="config-toggle">
              <button
                className={`toggle-btn ${config.algorithm === 'cfr_plus' ? 'active' : ''}`}
                onClick={() => setConfig((c) => ({ ...c, algorithm: 'cfr_plus' }))}
              >CFR+</button>
              <button
                className={`toggle-btn ${config.algorithm === 'dcfr' ? 'active' : ''}`}
                onClick={() => setConfig((c) => ({ ...c, algorithm: 'dcfr' }))}
              >DCFR</button>
            </div>

            <label className="config-label">Iterations</label>
            <input
              type="number"
              className="config-input"
              min={10}
              max={1000000}
              value={config.iterations}
              onChange={(e) => setConfig((c) => ({ ...c, iterations: Number(e.target.value) }))}
            />

            <label className="config-label">Starting Pot</label>
            <input
              type="number"
              className="config-input"
              min={1}
              value={config.starting_pot}
              onChange={(e) => setConfig((c) => ({ ...c, starting_pot: Number(e.target.value) }))}
            />

            <label className="config-label">Effective Stack</label>
            <input
              type="number"
              className="config-input"
              min={1}
              value={config.effective_stack}
              onChange={(e) => setConfig((c) => ({ ...c, effective_stack: Number(e.target.value) }))}
            />

            <label className="config-label">Max Raises/Street</label>
            <input
              type="number"
              className="config-input"
              min={0}
              max={5}
              value={config.max_raises}
              onChange={(e) => setConfig((c) => ({ ...c, max_raises: Number(e.target.value) }))}
            />
          </div>
        </section>

        {/* Actions */}
        <div className="action-row">
          <button className="btn-calculate" onClick={handleSolve} disabled={!canSolve}>
            {solving ? <><span className="spinner" />Solving…</> : 'Solve'}
          </button>
          <button className="btn-reset" onClick={handleReset}>Reset</button>
        </div>
      </div>

      {/* Right column: card picker + results */}
      <div className="right-column">
        <section className="card-section">
          <h2 className="section-title">
            Pick Board Cards
            <span className="hint"> ({boardCards.length}/{boardCount})</span>
          </h2>
          <CardGrid
            usedCards={usedCards}
            holeCards={new Set()}
            communityCards={new Set(boardCards)}
            selectionMode="community"
            onCardClick={handleBoardCardClick}
          />
        </section>

        {error && (
          <div className="error-banner">
            <strong>Error:</strong> {error}
          </div>
        )}

        {(solving || progress) && (
          <section className="card-section">
            <h2 className="section-title">Progress</h2>
            <ProgressBar progress={progress} onCancel={handleCancel} solving={solving} />
          </section>
        )}

        {result && (
          <section className="card-section">
            <h2 className="section-title">Results</h2>
            <StrategyDisplay result={result} />
          </section>
        )}
      </div>
    </main>
  );
}
