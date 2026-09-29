# Game Sample — ket-rs Demo Game

A small 2D arena game (Bevy) driven by the **ket** decision engine.
Reference: [katgpt-rs](https://github.com/lidm0707/katgpt-rs) — the upstream
project providing `katgpt-core` (salience gate, urgency probe bank, smooth-min
router) and `katgpt-sense` (`SectorProjection`) that ket-rs wraps.

Source: `examples/game_sample.rs` — run with
`cargo run --example game_sample`.

## 1. How to Play

| Element | What it does |
| --- | --- |
| **Player (blue)** | You. Move with **WASD / arrow keys**. Smooth inertia: speed ramps up and coasts down. |
| **Enemy (red)** | AI-driven hunter. It asks the ket engine *every tick* which behavior to run (Chase / Wander / Flee / Orbit / Evade). It has **10 HP** (yellow-damaged gauge above it) that drains over time — *time is HP*. |
| **Coins (yellow)** | 6 moving, bouncing hazards. **Collect all 6 to win.** They never disappear on their own. |
| **Player HP** | 3 hearts (gauge above the player). Enemy contact costs 1, with a 1.5 s invulnerable blink after each hit. 0 = lose. |

**Win condition:** collect all 6 coins → `YOU WIN`.
**Lose condition:** player HP reaches 0 → `YOU LOSE`.

**Controls**

| Key | Action |
| --- | --- |
| `WASD` / arrows | Move (full window range, eased movement) |
| `R` | New game (after win/lose) |
| `Esc` | Restart instantly, anytime |

**Extra systems**

- **Enemy time-as-HP:** bleeds 0.25 HP/s, blinks on each HP tick, respawns full when drained.
- **Coins hurt the enemy:** touching a coin costs the enemy 1 HP (1 s cooldown), with a soft push-out — it is never teleported, only nudged out over a few frames while keeping its AI direction.
- **Desperation sprint:** the lower the enemy HP, the faster it moves (up to 2×).
- **Movement graph:** fading breadcrumbs trace the enemy's path.

## 2. How katgpt (ket) Is Used

No training. No hand-coded `if/else` AI tree. The pipeline is
**rules → latent → decision**:

### Step 1 — Rules encode evidence into the latent vector

Every tick, `encode_state` writes 6 dims (fixed `STATE_DIM = 6`):

| Dim | Meaning | Rule |
| --- | --- | --- |
| 0 `threat` | 2.0 if player within 240 px | danger close |
| 1 `hurt` | 1.5 if player HP ≤ 1 **or** enemy HP < 30% | desperation |
| 2 `cornered` | 2.0 if threatened near a wall | trapped |
| 3 `loot` | 1.0 presence + up to 2.0 by closing speed, +2.0 panic bump if a coin is closing hard within 150 px | incoming hazard |
| 4 `coin_dx` | signed x of unit direction to nearest coin (inside 260 px) | bearing |
| 5 `coin_dy` | signed y of the same | bearing |

All dims are finite-checked by `KetConflictDetector` before use.

### Step 2 — The engine plans and decides

`KetQuery` asks `TypedQuestion::PickWeighted` with 5 choices and a
**per-choice direction bank** (`ACTION_DIRECTIONS`) — each action reads the
latent state through its own evidence profile:

| Action | Row (dim0..5) | Wins when |
| --- | --- | --- |
| `Chase` | `[0.9, 0, -0.3, -0.05, 0, 0]` | threat present, no danger — hunt the player |
| `Wander` | `[0,0,0,0,0,0]` | neutral state |
| `Flee` | `[0.9, 0.6, 0.6, -0.5, 0, 0]` | threatened + hurt/cornered — run |
| `Orbit` | `[-0.4, -0.4, -0.4, -0.3, 1.2, 1.2]` | calm coin nearby — circle it |
| `Evade` | `[0, 0, 0, 1.0, 0, 0]` | coin closing fast inside 150 px — dodge |

Scoring per choice (in `decision.rs`):
`0.5 × logistic(domain_score) + 0.5 × logistic(dot(dir_c, dims))` — the
argmax wins, so *changing the latent changes the chosen action directly*.
`domain_score` comes from the upstream ternary `SectorProjection` (Safety /
Time / Resource / General sectors — the Resource sector pushes on the coin
bearing dims), and the **urgency probe bank** tags Routine / Elevated /
Critical, which scales the enemy's aggression (×1.0 / 1.25 / 1.6).

### Step 3 — Movement from the decision

Each action maps to a steering direction: Chase = toward player, Flee = away
(+ center-ring clamp), Orbit = tangent around the nearest coin, Evade = run
from the coin's predicted position 1.3 s ahead (+ center pull so it never
gets cornered), Wander = rotating heading.

### Why this design

The whole AI is **data in the latent vector + direction rows** — new
behaviors are added by adding a row, not by writing branch logic. The same
6-float state drives scoring, urgency, and gating through the katgpt-rs
primitives (sigmoid dot-products, never softmax; zero-alloc hot path).
