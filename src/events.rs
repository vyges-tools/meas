//! The `vyges-events` causal trail for this engine.
//!
//! Every event goes to **stderr**; the result goes to stdout (or `-o`), so a caller can parse one
//! without the other. `code` is the clustering key — the thing you group by when the same failure
//! shows up across a hundred runs — and `objects` are the cross-stage co-reference keys, so a
//! measurement refusal here can be linked back to the simulation that produced the record.
//!
//! Codes are stable, and each names *a situation*, not a message:
//!
//! | code | meaning |
//! |---|---|
//! | `MEAS-SPECTRAL` | a spectral scalar was measured |
//! | `MEAS-AC` | an AC transfer scalar was measured |
//! | `MEAS-REFUSED` | the input could not support the requested method, and why |
//! | `MEAS-CLIPPED` | the record reaches the declared clip level |
//! | `MEAS-FOLDED` | a harmonic aliased back into the first Nyquist zone |
//! | `MEAS-DONE` | one invocation finished; carries the verdict |
//!
//! A refusal is emitted at `error` severity even though it is an expected outcome: from the
//! caller's side an unmeasurable input is something that needs attention, and burying it at
//! `info` would hide the one event worth grouping on.

use vyges_events::{emit, Event, Severity};

const TOOL: &str = "vyges-meas";

/// A spectral scalar was measured.
pub fn measured(metric: &str, db: f64, n: usize, fundamental_bin: usize) {
    emit(
        &Event::new(
            TOOL,
            Severity::Info,
            format!(
                "{metric} = {db:.4} dB ({n}-sample record, fundamental on bin {fundamental_bin})"
            ),
        )
        .with_code("MEAS-SPECTRAL")
        .with_objects(vec![
            format!("metric:{metric}"),
            format!("bin:{fundamental_bin}"),
        ]),
    );
}

/// An AC transfer scalar was measured.
pub fn ac_measured(metric: &str, value: f64, unit: &str, points: usize) {
    emit(
        &Event::new(
            TOOL,
            Severity::Info,
            format!("{metric} = {value:.6} {unit} (over {points} swept point(s))"),
        )
        .with_code("MEAS-AC")
        .with_objects(vec![format!("metric:{metric}")]),
    );
}

/// A harmonic aliased back into the first Nyquist zone.
///
/// Worth its own event rather than a footnote: a high-order harmonic landing on a low bin is the
/// usual explanation for a spur nobody expected, and seeing the fold in the trail is what turns
/// that from a mystery into a fact.
pub fn folded(order: usize, bin: usize) {
    emit(
        &Event::new(
            TOOL,
            Severity::Info,
            format!("harmonic {order} folds back to bin {bin} (above Nyquist)"),
        )
        .with_code("MEAS-FOLDED")
        .with_objects(vec![format!("harmonic:{order}"), format!("bin:{bin}")]),
    );
}

/// The record reaches the declared clip level.
pub fn clipped(index: usize, level: f64) {
    emit(
        &Event::new(
            TOOL,
            Severity::Warn,
            format!("sample {index} reaches the clip level {level} — the capture is clipped"),
        )
        .with_code("MEAS-CLIPPED")
        .with_objects(vec![format!("sample:{index}")]),
    );
}

/// The input could not support the requested method.
pub fn refused(metric: &str, why: &str) {
    emit(
        &Event::new(TOOL, Severity::Error, format!("{metric}: {why}"))
            .with_code("MEAS-REFUSED")
            .with_objects(vec![format!("metric:{metric}")]),
    );
}

/// One invocation finished. `verdict` is the engineering outcome as the result reports it, so the
/// trail and the envelope agree without a reader having to correlate them.
pub fn done(metric: &str, verdict: &str, detail: &str) {
    let sev = match verdict {
        "fail" => Severity::Warn,
        "unknown" => Severity::Error,
        _ => Severity::Info,
    };
    emit(
        &Event::new(TOOL, sev, format!("{metric} {verdict} — {detail}"))
            .with_code("MEAS-DONE")
            .with_objects(vec![format!("metric:{metric}")]),
    );
}
