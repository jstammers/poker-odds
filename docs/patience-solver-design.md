# Patience Solver Design — FreeCell & Klondike

Status: **design proposal, nothing implemented yet**
Scope: optimal (minimum-move) and satisficing solvers for FreeCell and Klondike
solitaire, plus proofs of unwinnability, as a new `src/patience/` module.

---

## 1. Prior art survey

### 1.1 Rust libraries — nothing reusable exists

| Crate / project | What it is | Verdict |
|---|---|---|
| [`freecell`](https://crates.io/crates/freecell) (`freecell-rs`) | Game objects and rules only. v0.1.0, last published Nov 2019, 7 recent downloads, 3 GitHub stars. Its own README lists "optimised structs for implementing a FreeCell solver" as *unimplemented*. | **No solver.** Unmaintained. Not worth a dependency. |
| [`lonelybot`](https://github.com/vuonghy2442/lonelybot) | MIT-licensed Rust engine for Thoughtful/Random Klondike. DFS + transposition tables, suit symmetry, dominance and move pruning; ~3 orders of magnitude faster than Solvitaire. Reports 81.95 ± 0.03 % thoughtful and 47.58 ± 0.80 % random winnability. | Klondike **only**, and it answers *is it winnable* — not *what is the shortest solution*. Best used as a **correctness oracle**, not a dependency. |
| `soliterm`, `klondike-rs` | Playable terminal games. | No solving. |
| `solitaire` (crates.io) | Unrelated (the Schneier Solitaire cipher). | Irrelevant. |

Non-Rust reference implementations worth reading:

- **[Freecell Solver](https://www.shlomifish.org/open-source/projects/freecell-solver/)** (Shlomi Fish, C) — the mature FreeCell solver. Supports DFS, randomised DFS, best-first search and an A\* scan with **configurable weights on state-evaluation parameters**, plus a post-hoc BrFS pass over intermediate states to shorten an already-found solution. Its architecture (many configurable scans, "switch-tasking" between them) is the model for our pluggable-search layer.
- **[Solvitaire](https://arxiv.org/abs/1906.12314)** (Blake & Gent, JAIR) — DFS with transposition tables, symmetry breaking, **dominances** and streamliners, parameterised over many patience games (Klondike, FreeCell, Spider, Canfield, Black Hole…). Contains the first correctness proofs of two key dominances. Source of our pruning rules.
- **[KlondikeSolver](https://github.com/shlomif/KlondikeSolver)**, [gingeleski/solitaire-solver](https://github.com/gingeleski/solitaire-solver) — minimal-length Klondike solutions.

### 1.2 Algorithms from the literature

**Complexity.** FreeCell is NP-complete for any fixed number of free cells
(Helmert, *Complexity Results for Standard Benchmark Domains in Planning*,
AIJ 143(2), 2003). Spider is NP-complete too
([arXiv:1110.1052](https://arxiv.org/abs/1110.1052)). So: no polynomial
algorithm; the game is won by strong pruning plus good heuristics, and
"optimal" means *proved optimal by exhaustive search under a bound*, not
*computed in closed form*.

**Satisficing FreeCell.** Heineman's **Staged Deepening** (HSD) is a hybrid
A\*/hill-climbing search: a local DFS runs from the head of the open list to
depth *k* with no heuristic evaluation, using a transposition table to avoid
loops; only nodes at exactly depth *k* enter the open list, ordered by
heuristic; when the table exceeds a size cap it is flushed wholesale. With
**HSDH** — for each foundation, find the next card it needs, count the cards
buried on top of it, sum over foundations — HSD solves **96 % of the Microsoft
32K suite**. Later genetic-programming work (GA-FreeCell, Elyasaf/Sipper et al.)
evolves weighted combinations of such heuristics and pushes this higher.

**Optimal FreeCell.** Paul & Helmert, *Optimal Solitaire Game Solutions Using
A\* Search and Deadlock Analysis* (SoCS/ICAPS 2016) — A\* with an **admissible**
heuristic derived from a directed graph whose **cycles represent deadlocks**;
reported as the first method to efficiently find *optimal* FreeCell solutions.
This is the target for our optimal mode.

**Klondike.** Bjarnason, Fern & Tadepalli:
[*Lower Bounding Klondike Solitaire with Monte-Carlo Planning*](https://ojs.aaai.org/index.php/ICAPS/article/view/13363)
(≥ 35 % win rate for a real, stochastic policy) and *Searching Solitaire in
Real Time*, which introduces a **search-tree compression** and the resulting
state representation for Thoughtful Solitaire, solving > 80 % of games in under
4 s and bounding winnability in [82 %, 91.44 %]. Solvitaire narrowed thoughtful
Klondike to 81.945 % ± 0.084 %; lonelybot to 81.95 % ± 0.03 %.

### 1.3 Conclusion

Nothing in Rust gives us shortest-solution search for either game. We build it
in-repo, reusing this crate's `cards` primitives, and borrow the algorithmic
content above: Solvitaire's dominances, HSD for the fast path, A\*/DFBnB with an
admissible heuristic for the optimal path.

---

## 2. Goals

1. **Optimal solver** — a provably minimum-cost solution for a given deal, with
   the cost model configurable (see §5.1).
2. **Fast solver** — find *some* solution quickly (target: full MS 32K suite in
   seconds, ≥ 99 % solved), used both standalone and as an upper bound to seed
   the optimal search.
3. **Winnability proof** — exhaust the (pruned, deduplicated) state space to
   prove a deal unsolvable. Ground truth: MS FreeCell deal **#11982**, the one
   unsolvable deal in the original 32,000.
4. **Both games** behind one search layer: FreeCell and Klondike (draw-1 and
   draw-3, thoughtful/open).
5. Runs **native and on `wasm32`**, matching the crate's existing dual-target
   discipline.

Explicit non-goals for v1: Spider, stochastic ("random", face-down-unknown)
Klondike play policies, and GPU/distributed search. §9 sketches where they'd go.

---

## 3. Module layout

Mirrors the existing `sim/` and `solver/` split (state ⟂ engine ⟂ config ⟂ result):

```
src/patience/
├── mod.rs            # re-exports: SolveConfig, SolveOutcome, Solution, solve()
├── deal.rs           # deal generators + layout parsing/serialisation
├── solution.rs       # Move lists, replay verification, pretty printing
├── game.rs           # `Patience` trait — the contract the search layer needs
├── freecell/
│   ├── mod.rs
│   ├── state.rs      # FcState, packing/canonicalisation
│   ├── moves.rs      # move generation, supermoves, apply/undo
│   ├── dominance.rs  # safe automoves + sound prunings
│   └── heuristic.rs  # admissible h + HSDH
├── klondike/
│   ├── mod.rs
│   ├── state.rs      # KlState, stock/waste cycle representation
│   ├── moves.rs      # macro "draw ×k then move" generation
│   ├── dominance.rs
│   └── heuristic.rs
└── search/
    ├── mod.rs
    ├── tt.rs         # transposition table (exact + digest modes)
    ├── astar.rs      # A* with bucket queue (optimal)
    ├── dfbnb.rs      # depth-first branch & bound (anytime optimal)
    ├── idastar.rs    # IDA* + TT (memory-lean optimal)
    └── staged.rs     # HSD / weighted best-first (satisficing)
```

Added to `src/lib.rs` as `pub mod patience;`.

### 3.1 The `Patience` trait

The search algorithms are written once against this trait, so both games (and
later Spider) share A\*, DFBnB, IDA\* and HSD.

```rust
pub trait Patience {
    type State: Clone;
    /// Compact, ideally `Copy`, move encoding.
    type Move: Copy + fmt::Debug;
    /// Canonical packed form used as the transposition-table key.
    type Key: Eq + Hash + Clone;

    fn initial(deal: &Deal) -> Self::State;
    fn is_goal(s: &Self::State) -> bool;

    /// Forced moves (dominances) applied to fixpoint before branching.
    /// Returns the moves it made so the solution can be reconstructed.
    fn apply_forced(s: &mut Self::State, out: &mut Vec<Self::Move>);

    /// Legal, non-dominated moves in a deterministic order.
    fn moves(s: &Self::State, out: &mut Vec<Self::Move>);

    fn apply(s: &mut Self::State, m: Self::Move) -> Undo;
    fn undo(s: &mut Self::State, u: Undo);

    fn cost(m: Self::Move, model: CostModel) -> u32;

    /// Canonical key: symmetry-reduced (§6.3).
    fn key(s: &Self::State) -> Self::Key;

    /// Admissible lower bound on remaining cost — required for optimality.
    fn h_admissible(s: &Self::State, model: CostModel) -> u32;
    /// Fast, inadmissible guidance for the satisficing search.
    fn h_greedy(s: &Self::State) -> u32;
}
```

`apply`/`undo` rather than clone-per-child: DFS-based searches (DFBnB, IDA\*,
HSD) dominate the runtime and must not allocate per node.

---

## 4. State representation

### 4.1 Reusing `cards`

`Card`, `Rank` and `Suit` are reused as the parsing/display layer. Two
adjustments are needed:

- **Ace is low in patience.** `Rank::Ace = 14` and `Rank::index()` yields
  `Two = 0 … Ace = 12`. Patience wants `A = 1 … K = 13`. Add a
  `patience_rank(Rank) -> u8` helper in `patience/mod.rs` — a mapping, not a
  change to `cards`, so poker code is untouched.
- **Internal card byte.** Search uses its own `u8` encoding
  `card = (suit << 4) | rank` (rank 1–13, suit 0–3), with `0xFF` for *empty*
  and the high bit reserved for *face-down* in Klondike. This makes
  "is red", "is one lower and opposite colour" single-instruction tests.
  Convert at the boundary via `From<Card>`.

### 4.2 FreeCell state

```rust
pub struct FcState {
    /// 8 cascades, each up to 52 cards; `len[i]` is the live length.
    cols: [[u8; 24]; 8],
    lens: [u8; 8],
    /// 4 free cells, EMPTY-padded, kept sorted ascending (canonical).
    cells: [u8; 4],
    /// foundations[suit] = highest rank placed, 0 = empty.
    found: [u8; 4],
}
```

24 is a safe per-column cap (a column can never exceed 20-odd cards in a legal
FreeCell position; the packer asserts it). Total ≈ 205 B — fine as the search
node, but too fat for the transposition table, hence the packed key in §6.

### 4.3 Klondike state

```rust
pub struct KlState {
    /// Face-up run per pile, top of pile last.
    piles: [[u8; 20]; 7],
    pile_len: [u8; 7],
    /// Count of still-face-down cards under each pile (thoughtful: their
    /// identity is known from the deal and recovered by index).
    down_len: [u8; 7],
    /// The stock as a fixed cyclic sequence + a cursor; waste is
    /// "everything before the cursor that has not been played".
    stock: [u8; 24],
    stock_len: u8,
    cursor: u8,
    found: [u8; 4],
}
```

The stock/waste pair is *not* stored as two lists. Following the
search-tree-compression idea from *Searching Solitaire in Real Time* and
lonelybot's encoding, the stock is one cyclic sequence plus a cursor, and the
solver never emits a bare "draw" move (see §5.3).

---

## 5. Moves

### 5.1 Cost model

```rust
pub enum CostModel {
    /// Each single-card relocation costs 1. A supermove of n cards costs n.
    /// Default: matches how most solvers report "shortest solution".
    CardMoves,
    /// A supermove costs 1 (what a UI player experiences as one drag).
    PlayerMoves,
    /// Klondike only: stock draws are free, tableau/foundation moves cost 1.
    IgnoreDraws,
}
```

"Optimal" is always relative to a declared cost model — the model is part of
`SolveConfig` and is echoed in `SolveOutcome`, because a solution optimal under
`CardMoves` is generally *not* optimal under `PlayerMoves`.

### 5.2 FreeCell moves

```rust
pub struct FcMove { from: Loc, to: Loc, count: u8 }
pub enum Loc { Col(u8), Cell(u8), Foundation(u8) }
```

Sources: 8 cascade tops + 4 free cells. Destinations: 8 cascades + 4
foundations + a free cell. Supermove capacity is the standard
**(1 + empty free cells) × 2^(empty cascades)** — halved when the destination
*is* an empty cascade, since that column can no longer be used as scratch.

Under `CardMoves`, a supermove is expanded into its constituent single-card
moves in the emitted solution, so the reported move count is honest and the
verifier can replay it against plain rules.

### 5.3 Klondike moves

Move kinds: waste→tableau, waste→foundation, tableau→tableau (a whole face-up
run or a suffix), tableau→foundation, foundation→tableau (needed for
completeness; heavily pruned).

Crucially, a move that consumes a stock card is emitted as a **macro move**
`Draw { times: k, then: Move }` costing *k + 1* (or 1 under `IgnoreDraws`).
Enumerating each individual draw as a search node explodes the branching factor
for no benefit; instead, move generation walks the cycle once and, for each
reachable waste card, records the minimum number of draws to expose it. With
draw-3 and unlimited redeals this is a single pass over ≤ 24 stock positions.

---

## 6. Pruning — where the wins are

Ordered by expected reduction. Everything in §6.1–6.3 is **sound** (preserves
optimality); §6.4 is opt-in and unsound.

### 6.1 Forced safe automoves (dominance)

A card is sent to its foundation *without branching* when doing so can never
hurt:

- **FreeCell:** rank ≤ 2 always; otherwise if both opposite-colour foundations
  are at rank ≥ r − 1 **and** the other same-colour foundation is at ≥ r − 2.
- **Klondike:** rank ≤ 2 always; otherwise if both opposite-colour foundations
  are at rank ≥ r − 1.

Applied to fixpoint in `apply_forced` before the state is ever hashed or
expanded. This also **shrinks the transposition table**: states in the middle of
a forced chain are never stored, because re-reaching such a state simply
re-runs the same forced chain to the same endpoint (the argument given in the
Solvitaire work).

*Implementation note:* both rules are the standard formulations and each will be
re-derived and unit-tested against a brute-force reference on small positions
before being trusted — an over-eager automove rule silently costs optimality.

### 6.2 Move-level dominances

- **No immediate inverse.** Never undo the previous move (kills all 2-cycles;
  with the other rules the search graph becomes a DAG).
- **Interchangeable empty destinations.** Consider at most *one* empty cascade
  and *one* empty free cell as a destination — the others are relabellings.
- **Partial-run moves (Solvitaire's proved dominance).** A proper suffix of a
  built pile may only be moved when the card newly exposed underneath can
  immediately be built to a foundation; otherwise moving the whole run is at
  least as good.
- **Cell→cell** moves are never generated.

### 6.3 Symmetry reduction (canonical keys)

The transposition key is built from a canonical form, not the raw state:

1. **Free cells sorted** ascending — cells are unlabelled.
2. **Columns sorted** by their packed byte string — the 8 FreeCell cascades are
   interchangeable. (Klondike piles are *not* freely interchangeable, because
   face-down counts matter; they are canonicalised only among empty piles.)
3. **Suit relabelling (opt-in).** Hearts↔Diamonds and Clubs↔Spades are
   automorphisms of the FreeCell rules and of the goal, so quotienting the
   search graph by the 4-element relabelling group is sound for shortest-path
   search. It requires composing the accumulated relabelling when reconstructing
   the solution path, so it ships behind a flag and defaults **on** for
   winnability mode, **off** for optimal mode until the path remapping is
   covered by tests.

Key packing: canonical form → a fixed-size `[u8; 64]` → hashed. Two TT modes:

- `TtMode::Exact` — stores the packed bytes. No false positives; required when
  *proving* a deal unsolvable, since a hash collision could otherwise prune a
  live branch and produce a wrong "unsolvable" verdict.
- `TtMode::Digest` — stores a 64-bit digest only (≈ 4× less memory, faster).
  Fine for finding solutions. `SolveOutcome::Unsolvable` is **only** ever
  returned from an `Exact` run.

Backing store: a fixed-capacity open-addressed table with an LRU-ish
replacement policy and a hard memory budget from `SolveConfig`, so a wasm build
cannot OOM the tab.

### 6.4 Streamliners (satisficing only)

Unsound restrictions that make hard deals fall quickly — e.g. forbidding
tableau→cell moves when a tableau→tableau move exists, or capping cell
occupancy. Used by the fast solver; a failure under streamliners falls back to
the unrestricted search rather than reporting "unsolvable".

---

## 7. Heuristics

### 7.1 Admissible (drives the optimal search)

Under `CostModel::CardMoves`:

```
h(s) = (52 − Σ foundation heights)          // every remaining card moves ≥ 1
     + |{ c : c lies above a lower-ranked card of the same suit
            in the same cascade }|            // those cards move ≥ 2
```

The second term is admissible because such a card must leave its column before
the buried card can be played, and must later move again to reach its
foundation — two distinct moves, and the first term only charged it one. Cards
in free cells contribute 1 (already counted). Cost is O(52) with a running
per-suit minimum, cheap enough for IDA\*.

**Phase 3 upgrade — deadlock analysis** (Paul & Helmert): build a digraph over
cards with an edge c → d when c must be moved before d (same-column burial,
plus foundation ordering). Each cycle forces an extra "park in a free cell"
move; a lower bound is extracted from a cycle cover. This is the difference
between "optimal on easy deals" and "optimal on the MS 32K suite", and is
scheduled once the simple bound is correct and benchmarked.

### 7.2 Inadmissible (drives the fast search)

**HSDH**: for each foundation, locate the next card it needs and count the cards
buried on top of it; sum over the four foundations. Extended, fc-solve style,
with a weighted sum over state-evaluation features — cards out of foundation,
buried-card depth, free cells used, empty columns, longest sorted run — with the
weights in a config struct so they can be tuned (and, later, evolved as in
GA-FreeCell) rather than hard-coded.

---

## 8. Search strategies

| Mode | Algorithm | Guarantee |
|---|---|---|
| `Fast` | Staged deepening (HSD) with HSDH + streamliners | A solution, usually fast |
| `Optimal` | Fast pass → upper bound `UB`; then DFBnB (or IDA\*) with `f = g + h` pruned at `UB`, iterating as `UB` improves | Minimum cost under the declared `CostModel`, or `Unsolvable` |
| `Winnable` | Exhaustive DFS + `TtMode::Exact` + all sound dominances | Solvable / **proved** unsolvable |

**Why DFBnB rather than plain A\*.** FreeCell optimal solutions run 50–60
single-card moves with a branching factor around 10–20 after pruning; a
best-first open list of that shape is memory-hostile, especially in wasm. DFBnB
with a transposition table keeps memory in the TT (which is explicitly budgeted)
and is naturally **anytime**: it emits a first solution early and a stream of
improving ones, then terminates having proved optimality when the frontier is
exhausted. A\* is still implemented for small/benchmark cases and to
cross-validate DFBnB's optimality claims on deals both can finish.

**Post-optimisation.** As fc-solve does, a found solution is passed through a
BrFS/shortcut pass over its own intermediate states — any state reachable twice
in the path collapses. Cheap, and it improves `Fast`-mode output without a full
optimal search.

**Budgets and cancellation.** Every search takes a node limit, a wall-clock
limit and a memory limit, and a `CancelFlag` in the style of
`sim::engine::CancelFlag`, so the TUI and the browser can interrupt. Exceeding a
budget yields `SolveOutcome::Unknown { best_so_far }` — never a false
`Unsolvable`.

**Parallelism.** Search itself stays single-threaded in v1 (shared TT
contention is a project of its own). Solving a *suite* of deals is embarrassingly
parallel and uses `rayon`, which is already a native-only dependency here; wasm
stays single-threaded.

---

## 9. Deals

`deal.rs` provides:

- **Microsoft deal numbers 1–32000** — the classic 32-bit LCG
  (`seed = seed * 214013 + 2531011`, `rand = (seed >> 16) & 0x7FFF`), dealing by
  swap-with-last from the shrinking deck. This gives us the canonical benchmark
  suite *and* interoperability with every other solver's deal numbering. Deal
  **#11982** is the known-unsolvable regression test.
- **Seeded random deals** via `rand_xoshiro`, already a dependency.
- **Text layout parsing/printing** in the standard fc-solve board format, so
  boards can be pasted in and solutions cross-checked against fc-solve and
  lonelybot.

---

## 10. Public API

```rust
pub enum Game { FreeCell { cells: u8, cascades: u8 }, Klondike { draw: u8, redeals: Redeals } }

pub struct SolveConfig {
    pub game: Game,
    pub mode: SolveMode,          // Fast | Optimal | Winnable
    pub cost: CostModel,
    pub max_nodes: Option<u64>,
    pub max_millis: Option<u64>,
    pub tt_bytes: usize,
    pub tt_mode: TtMode,
}

pub enum SolveOutcome {
    Solved { solution: Solution, optimal: bool, stats: SearchStats },
    Unsolvable { stats: SearchStats },
    Unknown { best: Option<Solution>, stats: SearchStats },
}

pub fn solve(deal: &Deal, cfg: &SolveConfig, cancel: &CancelFlag) -> SolveOutcome;
```

`Game::FreeCell` is parameterised on cell/cascade count so the 8×4 standard,
"Baker's Game"-style variants and easier 5-cell configurations all work — the
same parameterisation that lets Solvitaire cover a family of games.

`Solution` carries the move list, the cost under the declared model, and
`fn verify(&self, deal: &Deal) -> Result<(), VerifyError>` which replays it
against the plain rules with **all dominance and symmetry logic disabled**. Every
test and every benchmark run verifies its own output; a solver that prunes too
aggressively fails loudly rather than silently returning an illegal line.

---

## 11. Integration points

- **TUI** — a new screen under `src/tui/screens/` to enter or generate a deal
  and step through a solution.
- **WASM** — `#[wasm_bindgen]` wrappers in `src/wasm.rs` returning JSON
  solutions; the web app gets a solver page. Budgets matter more here: default
  to `Fast` mode with a conservative node limit and a small TT.
- **Benchmarks** — `benches/patience_bench.rs` (criterion, registered in
  `Cargo.toml` like the existing three): move generation throughput, nodes/sec,
  time-to-first-solution and time-to-optimal on a fixed deal sample.
- **CLI** — a `poker-odds patience` subcommand for batch suite runs.

No new runtime dependencies are required. `rustc-hash` (or an inlined FxHash) is
the one candidate addition, for the TT; it can be hand-rolled in ~20 lines if we
prefer to stay dependency-flat.

---

## 12. Testing

1. **Rule conformance** — hand-built positions per move type; property test that
   `apply` then `undo` restores the state byte-for-byte.
2. **Dominance safety** — on small/constrained positions, brute-force the full
   state space with dominances *off* and assert the optimal cost matches the run
   with them *on*. This is the test that protects optimality claims.
3. **Solution verification** — every solution replayed by the independent
   verifier (§10).
4. **Known ground truth** — MS deal #11982 must return `Unsolvable`; a sample of
   the MS 32K suite must all return `Solved`; aggregate winnability on random
   Klondike samples must sit inside the published 81.95 ± 0.03 % interval.
5. **Cross-validation** — spot-check optimal move counts against fc-solve and
   lonelybot on shared deal numbers (offline, not in CI).
6. **wasm** — `make check-wasm` must stay clean; no threads, no `getrandom`
   assumptions beyond the existing `wasm_js` setup.

---

## 13. Delivery plan

| Phase | Content | Rough size |
|---|---|---|
| 1 | `cards` bridge, `deal.rs` (MS generator + parser), `FcState`, move gen, `apply`/`undo`, `Solution` + verifier, tests | ~900 LoC |
| 2 | Dominances, canonical key, TT, HSD + HSDH → **Fast FreeCell**; MS 32K suite runner | ~700 LoC |
| 3 | Admissible heuristic, DFBnB/IDA\*/A\* → **Optimal FreeCell**; `Winnable` mode incl. #11982 | ~700 LoC |
| 4 | Deadlock-analysis heuristic; solution post-optimisation | ~400 LoC |
| 5 | Klondike state, macro draw moves, dominances, heuristics; both modes | ~900 LoC |
| 6 | TUI screen, WASM bindings, criterion bench, CLI subcommand | ~600 LoC |

Phases 1–3 are the useful minimum: a correct, provably optimal FreeCell solver.

---

## References

- Blake & Gent, *The Winnability of Klondike Solitaire and Many Other Patience Games* — [arXiv:1906.12314](https://arxiv.org/abs/1906.12314)
- Paul & Helmert, *Optimal Solitaire Game Solutions Using A\* Search and Deadlock Analysis*, SoCS 2016 — [ojs.aaai.org](https://ojs.aaai.org/index.php/SOCS/article/view/18405)
- Helmert, *Complexity Results for Standard Benchmark Domains in Planning*, AIJ 143(2), 2003
- Bjarnason, Fern & Tadepalli, *Lower Bounding Klondike Solitaire with Monte-Carlo Planning*, ICAPS 2009 — [ojs.aaai.org](https://ojs.aaai.org/index.php/ICAPS/article/view/13363)
- Bjarnason, Tadepalli & Fern, *Searching Solitaire in Real Time*, ICGA Journal
- Elyasaf, Hauptman & Sipper, *GA-FreeCell: Evolving Solvers for the Game of FreeCell*, GECCO 2011
- Fish, *Freecell Solver* — [shlomifish.org](https://www.shlomifish.org/open-source/projects/freecell-solver/)
- `lonelybot` — [github.com/vuonghy2442/lonelybot](https://github.com/vuonghy2442/lonelybot)
- `freecell-rs` — [github.com/Arman-Mielke/freecell-rs](https://github.com/Arman-Mielke/freecell-rs)
