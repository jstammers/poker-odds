import { useEffect, useState } from 'react';
import type { Backend } from './api/backend';
import { getBackend } from './api/backend';
import TabNav, { type Tab } from './components/TabNav';
import OddsCalculator from './pages/OddsCalculator';
import GtoSolver from './pages/GtoSolver';

export default function App() {
  const [backend, setBackend] = useState<Backend | null>(null);
  const [tab, setTab] = useState<Tab>('odds');
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    getBackend()
      .then(setBackend)
      .catch((e) => setError(e instanceof Error ? e.message : String(e)));
  }, []);

  if (error) {
    return (
      <div className="app">
        <div className="error-banner" style={{ margin: 20 }}>
          <strong>Failed to initialize:</strong> {error}
        </div>
      </div>
    );
  }

  if (!backend) {
    return (
      <div className="app">
        <div className="loading-splash">Loading…</div>
      </div>
    );
  }

  return (
    <div className="app">
      <header className="header">
        <div className="header-title">
          <span className="suit spades">♠</span>
          <span className="suit hearts">♥</span>
          <h1>Poker Odds</h1>
          <span className="suit diamonds">♦</span>
          <span className="suit clubs">♣</span>
        </div>
        <TabNav active={tab} onChange={setTab} showSolver={backend.hasSolver} />
      </header>

      {tab === 'odds' && <OddsCalculator backend={backend} />}
      {tab === 'solver' && backend.hasSolver && <GtoSolver backend={backend} />}
    </div>
  );
}
