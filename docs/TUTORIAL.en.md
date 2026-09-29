# ket-rs Tutorial: Rules → Latent → Decision (No Training)

> Educational walkthrough of `examples/rule_latent.rs`.
>
> Upstream substrate: [katgpt-rs](https://github.com/katopz/katgpt-rs)
> (`katgpt-core` / `katgpt-sense`) — every primitive (`SalienceTriGate`,
> sector projection) is imported from there.

---

## 1. The Big Picture

ket-rs "understands" text without training because meaning is
*declared by rules*, not learned from data. Each rule maps cue substrings
(letters) to a latent dim (numbers). The pipeline:

```
text ──encode──> [f32; 4] latent ──argmax──> class ──project+gate──> verdict
```

---

## 2. The Latent Vector

`STATE_DIM = 4` (`src/latent.rs`). The example assigns meaning to
each index:

| Index | Const       | Meaning |
| ----- | ----------- | ------- |
| 0     | `DIM_MEAL`  | cooked-meal evidence |
| 1     | `DIM_FRUIT` | raw-fruit evidence |
| 2     | `DIM_SWEET` | sweetness evidence |
| 3     | `DIM_HOT`   | served-hot evidence |

All values start at `0.0` and stay within `0.0..=EVIDENCE_MAX`.
Index = meaning because we name the consts ourselves — not a learned
embedding.

---

## 3. Constants: The Human-Chosen Scale

| Const | Value | Purpose |
| ----- | ----- | ------- |
| `RULE_HIT` | 0.5 | evidence bump per fired normal rule; more cues = more confidence |
| `ALERT_HIT` | 2.5 | safety-critical bump so risk always wins the argmax |
| `EVIDENCE_MAX` | 3.0 | per-dim ceiling; keeps every dim comparable and the gate stable |

Note the invariant: `ALERT_HIT + RULE_HIT = EVIDENCE_MAX`
(2.5 + 0.5 = 3.0). One safety alert plus one normal cue saturates a dim
exactly, so a safety signal can never be drowned out by ordinary evidence.

---

## 4. Step 1 — Encode

`encode()` checks every rule's cues with `text.contains(cue)`.
A rule fires **at most once** even if several cues match (`.any()`).
Each fired rule bumps its dim, capped:

```rust
fn bump(dims: &mut [f32; STATE_DIM], dim: usize, hit: f32) {
    dims[dim] = (dims[dim] + hit).min(EVIDENCE_MAX);
}
```

Worked example: `"hot rice and curry"`

| Rule | Cues found | Bump |
| ---- | ---------- | ---- |
| `cooked-staple` | `rice`, `curry` (2 cues, fires once) | `dims[0] = min(0.0 + 0.5, 3.0) = 0.5` |
| `served-hot` | `hot`, `curry` | `dims[3] = 0.5` |

Result: `dims = [0.5, 0.0, 0.0, 0.5]`, reasons = `["cooked-staple", "served-hot"]`

Worked example: `"a raw sweet banana"`

| Rule | Cue | Bump |
| ---- | --- | ---- |
| `raw-fruit` | `banana` | `dims[1] += 0.5` |
| `sweet-profile` | `sweet` | `dims[2] += 0.5` |
| `eaten-raw` | `raw` | `dims[1] += 0.5` → `1.0` |

Result: `dims = [0.0, 1.0, 0.5, 0.0]`

Reasons are the names of fired rules — 100% explainable because we
wrote the rules.

---

## 5. Step 2 — Classify by argmax

**argmax** = "argument of the maximum": the *index* of the largest
value (unlike `max`, which returns the value itself).
`classify()` compares only the class dims (0 and 1):

```rust
let mut best = (Class::Meal, f32::NEG_INFINITY);
for class in Class::ALL {
    let evidence = dims[class.dim()];
    if evidence > best.1 {
        best = (class, evidence);
    }
}
best.0
```

- `dims = [0.5, 0.0]` → argmax = index 0 → **Meal**
- `dims = [0.0, 1.0]` → argmax = index 1 → **Fruit**

Ties go to the first class (`>` not `>=`). The index *is* the meaning
because we named the dims ourselves. This is the bridge from "numbers"
back to "meaning".

---

## 6. Step 3 — Sector Projection

`KetScorer::project` (`src/score.rs`) takes dot products of the
latent with a hand-written **ternary direction bank** (`−1, 0, +1`):

```rust
const DIRECTIONS: [[i8; STATE_DIM]; SECTOR_COUNT] =
    [[1, 0, 0, -1],   // sector 0
     [0, 1, 1, 0],    // sector 1
     [0, 1, 0, 1],    // sector 2
     [-1, 0, 1, 1]];  // sector 3
```

For `dims = [0.0, 1.0, 0.5, 0.0]` (banana):

```
sector₀ = (+1)(0.0) + (0)(1.0) + (0)(0.5) + (−1)(0.0) = 0.0
sector₁ = (0)(0.0) + (+1)(1.0) + (+1)(0.5) + (0)(0.0) = 1.5
sector₂ = (0)(0.0) + (+1)(1.0) + (0)(0.5) + (+1)(0.0) = 1.0
sector₃ = (−1)(0.0) + (0)(1.0) + (+1)(0.5) + (+1)(0.0) = 0.5

sectors = [0.0, 1.5, 1.0, 0.5]
```

Purpose: mixing dims from 4 angles reveals *patterns* (e.g. sector₁ is high
when fruit and sweetness co-occur) instead of looking at raw class evidence.

---

## 7. Step 4 — The Emit Gate

`SalienceTriGate` (from
[`katgpt-core`](https://github.com/katopz/katgpt-rs)) multiplies the latent
by hand-tuned knob vectors
(`GATE_D_SPEAK = [0.9, 0.1, 0.0, −0.1]`, same layout as dims), pushes the
dot through two stacked sigmoids `1/(1+e^(−βx))` and compares against
`tau = 0.5` to choose Speak / Delegate / Silent:

```
"hot rice and curry": dot = 0.9(0.5) + 0.1(0) + 0(0) + (−0.1)(0.5) = 0.40
σ(6 × 0.40) = σ(2.4) ≈ 0.917 > 0.5  →  Speak
```

`GATE_D_SPEAK` encodes the author's *intent*: cooked meals strongly justify
speaking (0.9), fruit barely matters (0.1), heat pushes back slightly (−0.1).
No formula produced these numbers — they are hand-tuned, like knobs.

---

## 8. Why No Training Is Needed

| | Neural LM (trained) | ket-rs (rules) |
| --- | --- | --- |
| Source of meaning | learned from billions of tokens | humans write cue → dim rules |
| Numbers ↔ meaning | opaque learned embedding | named dims, exact mapping |
| Unseen words | may generalize | unknown cue = no fire = no meaning |

Every number in the system (`RULE_HIT`, `DIRECTIONS`, `GATE_D_*`)
is a human-written const. That is the trade-off: full explainability and
zero training cost, but no generalization beyond the written cues
(e.g. "ข้าวผัด" contains no "rice" and cannot be classified).

---

## 9. Try It

```sh
cargo run --example rule_latent --release
```

The example prints the latent and sectors for every test text.
Experiments to try: change `ALERT_HIT` to `1.0` and watch
`"spoiled rice and ripe banana"` become unstable; add a Thai cue
(`"ข้าว"` → `DIM_MEAL`) to feel how rule coverage defines understanding.
