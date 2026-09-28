# ket-rs

Rules → latent → decision, with **no training and no hand-coded dims**.

`ket-rs` turns symbolic rules over text into a small evidence vector (the
latent), then asks a decision engine to act on it. The latent is *derived*
from which rules fired, so the reasons for a decision are always the fired
rules themselves — explainability is a by-construction property, not a
post-hoc explanation.

## How it works

```mermaid
graph LR
    A[Text tokens] --> B[Rules fire on cues]
    B --> C[Evidence bumps in latent dims]
    C --> D[Argmax over class dims]
    D --> E[KetEngine decision]
```

1. **Encode** — a `Rule` is a set of cue substrings plus a latent dim. For
   every rule whose cues appear in the text, that dim gains evidence
   (`+RULE_HIT`, capped at `EVIDENCE_MAX`).
2. **Classify** — argmax evidence across the class dims picks a class
   (e.g. `meal` vs `fruit`).
3. **Decide** — the latent is projected into sectors (`KetScorer::project`)
   and wrapped in a `LatentState` / `KetQuery`, which `KetEngine::decide_into`
   turns into a verdict: urgency, emit flag, or a rejection with a reason.

Because rules write to named dims, the reasons are exactly the fired rule
names — no separate explanation pass needed.

## Quick start

```sh
cargo run --example rule_latent --release
```

Sample output:

```text
hot rice and curry: -> meal because ["cooked-staple", "served-hot"] ...
a raw sweet banana: -> fruit because ["raw-fruit", "sweet-profile", "eaten-raw"] ...
```

## Example: `examples/rule_latent.rs`

The example defines two classes (`Meal`, `Fruit`) and six keyword rules:

| Rule            | Cues                                | Dim          |
| --------------- | ----------------------------------- | ------------ |
| `cooked-staple` | rice, noodle, soup, curry, pork…    | `DIM_MEAL`   |
| `raw-fruit`     | banana, mango, apple, berry         | `DIM_FRUIT`  |
| `sweet-profile` | sweet, ripe, juicy                  | `DIM_SWEET`  |
| `served-hot`    | hot, grill, soup, curry             | `DIM_HOT`    |
| `eaten-raw`     | raw, fresh                          | `DIM_FRUIT`  |
| `dessert-form`  | pie, dessert                        | `DIM_SWEET`  |

Dims `0..CLASS_COUNT` are class evidence; the rest are context dims that the
scorer's sector projection consumes.

## Core API surface

| Module      | Types                                       | Role                                    |
| ----------- | ------------------------------------------- | --------------------------------------- |
| `latent`    | `LatentState`, `STATE_DIM`, `FLAG_SAFE`     | Fixed-size evidence vector + flags      |
| `score`     | `KetScorer`, `SECTOR_COUNT`                 | Projects latents into sector space      |
| `question`  | `TypedQuestion`, `ChoiceId`                 | Typed query (e.g. `Pick` with choices)  |
| `decision`  | `KetEngine`, `KetQuery`                     | Verdict engine: urgency / emit / reject |

Typical flow:

```rust
let (dims, reasons) = encode(text);          // rules -> latent
let class = classify(&dims);                 // argmax over class dims
let state = LatentState { dims: &dims, flags: FLAG_SAFE };
let query = KetQuery {
    state: &state,
    question: TypedQuestion::Pick { keywords: &KEYWORDS, choices: &[class.choice()] },
};
let verdict = engine.decide_into(query, &mut choices_out, &mut scores_out);
```

Note the zero-copy style: `LatentState` and `KetQuery` borrow the latent
slices, and `decide_into` writes into caller-provided output buffers.

## Modules

| Module       | Purpose                                             |
| ------------ | --------------------------------------------------- |
| `latent`     | Latent state representation and flags               |
| `score`      | Sector projection and scoring                       |
| `question`   | Typed question / choice encoding                    |
| `decision`   | Decision engine and verdicts                        |
| `probe`      | Indicator probes over the latent                    |
| `router`     | Routing between decision paths                      |
| `pruner`     | Pruning low-evidence dims                           |
| `exploration`| Bandit-style exploration                            |
| `conformal`  | Conformal predictive intervals (feature `ket_conformal`) |

## Features

- `ket` (default) — core engine (`katgpt-core`, `katgpt-sense`, `fastrand`)
- `ket_conformal` — adds conformal predictive intervals
