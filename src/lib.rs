//! ket: a modelless, structured decision engine.
//!
//! (program state, typed question) -> { choice, scores, confidence, urgency }.
//! No text generation, no training. All primitives are imported from the
//! katgpt-rs substrate (katgpt-core / katgpt-sense); this crate only maps
//! ket's domain types onto them.

pub mod latent;
pub mod question;

#[cfg(feature = "ket")]
pub mod decision;
#[cfg(feature = "ket")]
pub mod probe;
#[cfg(feature = "ket")]
pub mod pruner;
#[cfg(feature = "ket")]
pub mod router;
#[cfg(feature = "ket")]
pub mod score;

#[cfg(feature = "ket")]
pub mod exploration;

#[cfg(feature = "ket_conformal")]
pub mod conformal;
