export type Tab = 'odds' | 'solver';

interface Props {
  active: Tab;
  onChange: (tab: Tab) => void;
  showSolver: boolean;
}

export default function TabNav({ active, onChange, showSolver }: Props) {
  return (
    <nav className="tab-nav">
      <button
        className={`tab-btn ${active === 'odds' ? 'active' : ''}`}
        onClick={() => onChange('odds')}
      >
        Odds Calculator
      </button>
      {showSolver && (
        <button
          className={`tab-btn ${active === 'solver' ? 'active' : ''}`}
          onClick={() => onChange('solver')}
        >
          GTO Solver
        </button>
      )}
    </nav>
  );
}
