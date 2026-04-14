import { cardId, RANKS, SUITS, SUIT_GLYPHS } from '../types/odds';
import type { Rank, Suit } from '../types/odds';

interface Props {
  usedCards: Set<string>;
  holeCards: Set<string>;
  communityCards: Set<string>;
  selectionMode: 'hole' | 'community';
  onCardClick: (cardId: string) => void;
}

export default function CardGrid({
  usedCards,
  holeCards,
  communityCards,
  selectionMode,
  onCardClick,
}: Props) {
  return (
    <div className="card-grid" role="group" aria-label="Card picker">
      {/* Rank headers */}
      <div className="card-grid-header">
        <div className="grid-corner" />
        {RANKS.map((r) => (
          <div key={r} className="rank-label">{r === 'T' ? '10' : r}</div>
        ))}
      </div>

      {/* Rows: one per suit */}
      {SUITS.map((suit: Suit) => {
        const isRedSuit = suit === 'h' || suit === 'd';
        return (
          <div key={suit} className="card-row">
            <div className={`suit-label ${isRedSuit ? 'red' : 'black'}`}>
              {SUIT_GLYPHS[suit]}
            </div>
            {RANKS.map((rank: Rank) => {
              const id = cardId(rank, suit);
              const isHole = holeCards.has(id);
              const isCommunity = communityCards.has(id);
              const isUsed = usedCards.has(id);

              // Determine visual state
              let state: 'hole' | 'community' | 'used-other' | 'free' = 'free';
              if (isHole) state = 'hole';
              else if (isCommunity) state = 'community';
              else if (isUsed) state = 'used-other';

              // Can the user click this card?
              const isOtherCategoryUsed =
                (selectionMode === 'hole' && isCommunity) ||
                (selectionMode === 'community' && isHole);
              const disabled = isOtherCategoryUsed;

              return (
                <button
                  key={id}
                  className={`playing-card ${state} ${isRedSuit ? 'red' : 'black'} ${disabled ? 'disabled' : ''}`}
                  onClick={() => !disabled && onCardClick(id)}
                  disabled={disabled}
                  aria-label={`${rank} of ${suit}`}
                  aria-pressed={isHole || isCommunity}
                >
                  <span className="card-rank">{rank === 'T' ? '10' : rank}</span>
                  <span className="card-suit-small">{SUIT_GLYPHS[suit]}</span>
                  {(isHole || isCommunity) && (
                    <span className="card-role-badge">
                      {isHole ? 'H' : 'B'}
                    </span>
                  )}
                </button>
              );
            })}
          </div>
        );
      })}
    </div>
  );
}
