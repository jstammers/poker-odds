import type { VariantInfo } from '../types/odds';

interface Props {
  variants: VariantInfo[];
  selected: VariantInfo;
  onChange: (v: VariantInfo) => void;
}

export default function VariantPicker({ variants, selected, onChange }: Props) {
  return (
    <div className="variant-picker">
      {variants.map((v) => (
        <button
          key={v.id}
          className={`variant-btn ${v.id === selected.id ? 'active' : ''}`}
          onClick={() => onChange(v)}
          title={v.description}
        >
          {v.name}
        </button>
      ))}
    </div>
  );
}
