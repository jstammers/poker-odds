import { useCallback, useState } from 'react';
import type { Backend } from '../api/backend';

interface Props {
  label: string;
  value: string;
  onChange: (value: string) => void;
  backend: Backend;
  placeholder?: string;
}

export default function RangeInput({ label, value, onChange, backend, placeholder }: Props) {
  const [combos, setCombos] = useState<number | null>(null);
  const [error, setError] = useState<string | null>(null);

  const validate = useCallback(async (text: string) => {
    if (!text.trim()) {
      setCombos(null);
      setError(null);
      return;
    }
    try {
      const count = await backend.validateRange(text.trim());
      setCombos(count);
      setError(null);
    } catch (e) {
      setCombos(null);
      setError(e instanceof Error ? e.message : String(e));
    }
  }, [backend]);

  return (
    <div className="range-input">
      <label className="range-label">{label}</label>
      <input
        type="text"
        className={`range-field ${error ? 'range-error' : ''}`}
        value={value}
        placeholder={placeholder ?? 'AA,AKs,QQ-TT,AJs-A9s'}
        onChange={(e) => onChange(e.target.value)}
        onBlur={() => validate(value)}
      />
      <div className="range-info">
        {combos !== null && <span className="range-combos">{combos} combos</span>}
        {error && <span className="range-error-text">{error}</span>}
        {!combos && !error && value === '' && (
          <span className="range-hint">Leave empty for all hands</span>
        )}
      </div>
    </div>
  );
}
