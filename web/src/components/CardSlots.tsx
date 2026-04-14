import { parseCard, SUIT_GLYPHS } from '../types/odds';

interface Props {
  label: string;
  cards: string[];
  count: number;
  active: boolean;
  onClick: () => void;
  onRemove: (card: string) => void;
}

export default function CardSlots({ label, cards, count, active, onClick, onRemove }: Props) {
  const slots = Array.from({ length: count });

  return (
    <div className={`card-slots ${active ? 'active' : ''}`} onClick={onClick} role="button" tabIndex={0}>
      <div className="card-slots-label">{label}</div>
      <div className="card-slots-row">
        {slots.map((_, i) => {
          const cardId = cards[i];
          if (cardId) {
            const parsed = parseCard(cardId);
            const isRed = parsed?.suit === 'h' || parsed?.suit === 'd';
            const glyph = parsed ? SUIT_GLYPHS[parsed.suit] : '';
            return (
              <button
                key={i}
                className={`card-chip filled ${isRed ? 'red' : 'black'}`}
                onClick={(e) => { e.stopPropagation(); onRemove(cardId); }}
                title={`Remove ${cardId}`}
              >
                {parsed?.rank}{glyph}
              </button>
            );
          }
          return <div key={i} className="card-chip empty" />;
        })}
      </div>
    </div>
  );
}
