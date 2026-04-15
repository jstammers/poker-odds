import { useCallback, useRef, useState } from 'react';
import type { OddsResult, SimInput, VariantInfo } from '../types/odds';
import type { Backend } from '../api/backend';
import VariantPicker from '../components/VariantPicker';
import CardGrid from '../components/CardGrid';
import CardSlots from '../components/CardSlots';
import OddsDisplay from '../components/OddsDisplay';

type SelectionMode = 'hole' | 'community';

const DEFAULT_VARIANTS: VariantInfo[] = [
  { id: 'texas_holdem', name: "Texas Hold'em", description: '2 hole cards + 5 community.', hole_card_count: 2, community_card_count: 5, max_players: 9 },
  { id: 'omaha_holdem', name: "Omaha Hold'em", description: '4 hole cards + 5 community.', hole_card_count: 4, community_card_count: 5, max_players: 9 },
  { id: 'seven_card_stud', name: '7-Card Stud', description: '7 individual cards, no board.', hole_card_count: 7, community_card_count: 0, max_players: 7 },
  { id: 'five_card_draw', name: '5-Card Draw', description: '5 hole cards, no board.', hole_card_count: 5, community_card_count: 0, max_players: 6 },
];

interface Props {
  backend: Backend;
}

export default function OddsCalculator({ backend }: Props) {
  const variants = DEFAULT_VARIANTS;
  const [variant, setVariant] = useState<VariantInfo>(DEFAULT_VARIANTS[0]);
  const [holeCards, setHoleCards] = useState<string[]>([]);
  const [communityCards, setCommunityCards] = useState<string[]>([]);
  const [opponentCount, setOpponentCount] = useState(1);
  const [selectionMode, setSelectionMode] = useState<SelectionMode>('hole');
  const [iterations, setIterations] = useState(500_000);
  const [calculating, setCalculating] = useState(false);
  const [result, setResult] = useState<OddsResult | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [showSettings, setShowSettings] = useState(false);
  const pendingId = useRef(0);

  const usedCards = new Set([...holeCards, ...communityCards]);

  const handleCardClick = useCallback(
    (cardId: string) => {
      if (selectionMode === 'hole') {
        if (holeCards.includes(cardId)) {
          setHoleCards((prev) => prev.filter((c) => c !== cardId));
        } else if (holeCards.length < variant.hole_card_count) {
          setHoleCards((prev) => [...prev, cardId]);
        }
      } else {
        if (!variant.community_card_count) return;
        if (communityCards.includes(cardId)) {
          setCommunityCards((prev) => prev.filter((c) => c !== cardId));
        } else if (communityCards.length < variant.community_card_count) {
          setCommunityCards((prev) => [...prev, cardId]);
        }
      }
    },
    [selectionMode, holeCards, communityCards, variant],
  );

  const handleVariantChange = useCallback((v: VariantInfo) => {
    setVariant(v);
    setHoleCards([]);
    setCommunityCards([]);
    setResult(null);
    setError(null);
    setSelectionMode('hole');
  }, []);

  const handleReset = useCallback(() => {
    setHoleCards([]);
    setCommunityCards([]);
    setResult(null);
    setError(null);
    setSelectionMode('hole');
  }, []);

  const canCalculate = holeCards.length === variant.hole_card_count && !calculating;

  const handleCalculate = useCallback(async () => {
    if (!canCalculate) return;
    setCalculating(true);
    setResult(null);
    setError(null);

    const id = ++pendingId.current;

    const input: SimInput = {
      variant: variant.id,
      hole_cards: holeCards,
      community_cards: communityCards,
      opponent_count: opponentCount,
      iterations,
    };

    try {
      const res = await backend.calculateOdds(input);
      if (id === pendingId.current) {
        setResult(res);
        setError(null);
      }
    } catch (e) {
      if (id === pendingId.current) {
        setError(e instanceof Error ? e.message : String(e));
        setResult(null);
      }
    } finally {
      if (id === pendingId.current) {
        setCalculating(false);
      }
    }
  }, [canCalculate, variant, holeCards, communityCards, opponentCount, iterations, backend]);

  const holeComplete = holeCards.length === variant.hole_card_count;
  const communityMax = variant.community_card_count;

  return (
    <>
      {/* Settings toggle */}
      <div className="settings-toggle-row">
        <button className="btn-icon" onClick={() => setShowSettings((s) => !s)} title="Settings">
          ⚙
        </button>
      </div>

      {showSettings && (
        <div className="settings-panel">
          <h3>Settings</h3>
          <label>
            Monte Carlo Iterations
            <input
              type="range"
              min={10000}
              max={1000000}
              step={10000}
              value={iterations}
              onChange={(e) => setIterations(Number(e.target.value))}
            />
            <span className="settings-value">{iterations.toLocaleString()}</span>
          </label>
          <label>
            Opponents
            <input
              type="number"
              min={1}
              max={variant.max_players - 1}
              value={opponentCount}
              onChange={(e) => setOpponentCount(Number(e.target.value))}
            />
          </label>
        </div>
      )}

      <main className="main">
        <div className="setup-column">
          <section className="card-section">
            <h2 className="section-title">Game</h2>
            <VariantPicker variants={variants} selected={variant} onChange={handleVariantChange} />
          </section>

          <section className="card-section">
            <h2 className="section-title">Your Cards</h2>
            <CardSlots
              label="Hole Cards"
              cards={holeCards}
              count={variant.hole_card_count}
              active={selectionMode === 'hole'}
              onClick={() => setSelectionMode('hole')}
              onRemove={(c) => setHoleCards((prev) => prev.filter((x) => x !== c))}
            />
            {communityMax > 0 && (
              <CardSlots
                label="Board"
                cards={communityCards}
                count={communityMax}
                active={selectionMode === 'community'}
                onClick={() => setSelectionMode('community')}
                onRemove={(c) => setCommunityCards((prev) => prev.filter((x) => x !== c))}
              />
            )}
            {!showSettings && (
              <div className="opponent-row">
                <span className="label">Opponents</span>
                <button className="btn-stepper" onClick={() => setOpponentCount((n) => Math.max(1, n - 1))} disabled={opponentCount <= 1}>−</button>
                <span className="opponent-count">{opponentCount}</span>
                <button className="btn-stepper" onClick={() => setOpponentCount((n) => Math.min(variant.max_players - 1, n + 1))} disabled={opponentCount >= variant.max_players - 1}>+</button>
              </div>
            )}
          </section>

          <div className="mode-switcher">
            <button className={`mode-btn ${selectionMode === 'hole' ? 'active' : ''}`} onClick={() => setSelectionMode('hole')}>
              Select Hole Cards
              {holeComplete && <span className="checkmark">✓</span>}
            </button>
            {communityMax > 0 && (
              <button
                className={`mode-btn ${selectionMode === 'community' ? 'active' : ''}`}
                onClick={() => setSelectionMode('community')}
                disabled={!holeComplete}
              >
                Select Board
                {communityCards.length > 0 && <span className="badge">{communityCards.length}/{communityMax}</span>}
              </button>
            )}
          </div>

          <div className="action-row">
            <button className="btn-calculate" onClick={handleCalculate} disabled={!canCalculate}>
              {calculating ? <><span className="spinner" />Calculating…</> : 'Calculate Odds'}
            </button>
            <button className="btn-reset" onClick={handleReset}>Reset</button>
          </div>
        </div>

        <div className="right-column">
          <section className="card-section">
            <h2 className="section-title">
              {selectionMode === 'hole' ? 'Pick Hole Cards' : 'Pick Board Cards'}
              <span className="hint">
                {selectionMode === 'hole'
                  ? ` (${holeCards.length}/${variant.hole_card_count})`
                  : ` (${communityCards.length}/${communityMax})`}
              </span>
            </h2>
            <CardGrid
              usedCards={usedCards}
              holeCards={new Set(holeCards)}
              communityCards={new Set(communityCards)}
              selectionMode={selectionMode}
              onCardClick={handleCardClick}
            />
          </section>

          {error && (
            <div className="error-banner">
              <strong>Error:</strong> {error}
            </div>
          )}

          {(result || calculating) && (
            <section className="card-section">
              <h2 className="section-title">Results</h2>
              <OddsDisplay result={result} calculating={calculating} />
            </section>
          )}
        </div>
      </main>
    </>
  );
}
