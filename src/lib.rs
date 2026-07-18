//! vyges-meas — **closed measurement kernels** for analog characterization.
//!
//! Two families, each one *complete definition* rather than a family of options:
//!
//! - [`spectral`] — coherent single-tone **SNR, SINAD, THD, SFDR** from a captured time series;
//! - [`transfer`] — **gain, bandwidth, unity-gain frequency, phase margin** from an AC sweep.
//!
//! # Why the definitions are the product
//!
//! `SNR` names a ratio, not a measurement. Whether the fundamental is excluded from noise, how
//! many harmonics are counted, whether DC is in the band, where the band ends, what happens to a
//! harmonic that aliases — every one of those changes the number, and two honest tools can
//! differ by several dB while both being "right". A converter datasheet is only comparable to a
//! simulation if both state the same choices.
//!
//! So each module fixes one method, documents every choice, and **refuses inputs it cannot
//! measure that way** — a non-coherent capture, a clipped record, a harmonic colliding with the
//! fundamental. A refusal is a result: it says the number would have been meaningless.
//!
//! # Not a standards claim
//!
//! IEEE 1241 (ADCs), 1658 (DACs) and 1057 (waveform recorders) are the applicable references.
//! Their clause-level requirements have **not** been reviewed against this code, so nothing here
//! may be labelled as conforming to them. These are Vyges definitions: complete, reproducible,
//! and explicit about which they are.
//!
//! That is not left to prose. Every result carries an [`alignment`] claim, and the two strongest
//! rungs of that ladder can only be built from a recorded crosswalk — so a stronger claim than
//! the evidence supports has no constructor, let alone a default.

pub mod alignment;
pub mod events;
pub mod job;
pub mod spectral;
pub mod transfer;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const COPYRIGHT: &str = "Copyright (c) 2026 Vyges — Apache-2.0";
