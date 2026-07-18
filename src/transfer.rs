//! AC transfer metrics — **gain, bandwidth, unity-gain frequency, phase margin**.
//!
//! Same discipline as [`crate::spectral`]: each metric is one closed definition, stated here, and
//! an input that cannot support it is refused rather than extrapolated.
//!
//! The input is a frequency sweep of `(frequency, gain_db, phase_deg)` points, strictly
//! increasing in frequency. Between two bracketing points a value is found by **linear
//! interpolation in (log10 f, dB)** and in `(log10 f, degrees)` — the space these quantities are
//! plotted and reasoned about in, so a coarse decade sweep does not read low.
//!
//! Nothing is extrapolated. If a crossing does not occur between the first and last point of the
//! sweep, that is reported as not found; inventing the answer beyond the measured range is how a
//! sweep that stopped too early turns into a confident wrong number.
//!
//! | metric | definition |
//! |---|---|
//! | `gain` | gain in dB at the lowest frequency in the sweep (the DC-most point measured) |
//! | `bandwidth` | first frequency above the peak where gain falls 3 dB below the **peak** gain |
//! | `unity-frequency` | first frequency where gain crosses 0 dB going down |
//! | `phase-margin` | 180° + phase at the unity-gain frequency |
//!
//! Bandwidth is referenced to the **peak** rather than to the first point: a response with gain
//! peaking (a lightly damped second-order loop) has its −3 dB point relative to that peak, and
//! referencing the first point instead would report a bandwidth that is simply wrong for exactly
//! the circuits where the number matters most.

use crate::events;

/// One point of a measured frequency response.
#[derive(Clone, Copy, Debug)]
pub struct Point {
    pub hz: f64,
    pub gain_db: f64,
    pub phase_deg: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AcMetric {
    Gain,
    Bandwidth,
    UnityFrequency,
    PhaseMargin,
}

impl AcMetric {
    pub fn parse(s: &str) -> Option<AcMetric> {
        match s.to_ascii_lowercase().as_str() {
            "gain" => Some(AcMetric::Gain),
            "bandwidth" | "bw" => Some(AcMetric::Bandwidth),
            "unity-frequency" | "unity" => Some(AcMetric::UnityFrequency),
            "phase-margin" | "pm" => Some(AcMetric::PhaseMargin),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            AcMetric::Gain => "gain",
            AcMetric::Bandwidth => "bandwidth",
            AcMetric::UnityFrequency => "unity-frequency",
            AcMetric::PhaseMargin => "phase-margin",
        }
    }

    /// The unit the value carries, for the result record.
    pub fn unit(self) -> &'static str {
        match self {
            AcMetric::Gain => "dB",
            AcMetric::Bandwidth | AcMetric::UnityFrequency => "Hz",
            AcMetric::PhaseMargin => "deg",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Refusal {
    /// Fewer than two points — nothing to interpolate between.
    TooFewPoints(usize),
    /// Frequencies are not strictly increasing, or one is not positive.
    NotMonotonic(usize),
    /// A value is NaN or infinite.
    NotFinite(usize),
    /// The crossing this metric is defined by does not occur inside the swept range.
    NoCrossing { metric: &'static str, what: String },
}

impl std::fmt::Display for Refusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Refusal::TooFewPoints(n) => write!(f, "{n} point(s) — a sweep needs at least two"),
            Refusal::NotMonotonic(i) => write!(
                f,
                "point {i} does not increase in frequency (or is not positive) — the sweep must \
                 be ordered for interpolation to mean anything"
            ),
            Refusal::NotFinite(i) => write!(f, "point {i} holds a non-finite value"),
            Refusal::NoCrossing { metric, what } => write!(
                f,
                "{metric}: {what} does not occur within the swept range — extending the sweep is \
                 the fix; extrapolating past it would be a guess"
            ),
        }
    }
}

#[derive(Clone, Debug)]
pub struct AcMeasurement {
    pub metric: AcMetric,
    pub value: f64,
    pub unit: &'static str,
    pub points: usize,
    /// Peak gain and where it occurs — the reference bandwidth is measured from.
    pub peak_db: f64,
    pub peak_hz: f64,
}

/// Interpolate `y` at the crossing of `target`, in (log10 f, y) space, between two points.
fn cross(f0: f64, y0: f64, f1: f64, y1: f64, target: f64) -> f64 {
    let (l0, l1) = (f0.log10(), f1.log10());
    if (y1 - y0).abs() < f64::EPSILON {
        return f0;
    }
    let t = (target - y0) / (y1 - y0);
    10f64.powf(l0 + t * (l1 - l0))
}

/// Value of `y` at frequency `hz`, interpolated in (log10 f, y).
fn at(pts: &[Point], hz: f64, y: impl Fn(&Point) -> f64) -> f64 {
    if hz <= pts[0].hz {
        return y(&pts[0]);
    }
    if hz >= pts[pts.len() - 1].hz {
        return y(&pts[pts.len() - 1]);
    }
    for w in pts.windows(2) {
        if hz >= w[0].hz && hz <= w[1].hz {
            let (l0, l1) = (w[0].hz.log10(), w[1].hz.log10());
            let t = if (l1 - l0).abs() < f64::EPSILON {
                0.0
            } else {
                (hz.log10() - l0) / (l1 - l0)
            };
            return y(&w[0]) + t * (y(&w[1]) - y(&w[0]));
        }
    }
    y(&pts[pts.len() - 1])
}

fn validate(pts: &[Point]) -> Result<(), Refusal> {
    if pts.len() < 2 {
        return Err(Refusal::TooFewPoints(pts.len()));
    }
    for (i, p) in pts.iter().enumerate() {
        if !p.hz.is_finite() || !p.gain_db.is_finite() || !p.phase_deg.is_finite() {
            return Err(Refusal::NotFinite(i));
        }
        if p.hz <= 0.0 || (i > 0 && p.hz <= pts[i - 1].hz) {
            return Err(Refusal::NotMonotonic(i));
        }
    }
    Ok(())
}

/// The first downward 0 dB crossing, or `None` if the gain never crosses inside the sweep.
fn unity_hz(pts: &[Point]) -> Option<f64> {
    pts.windows(2)
        .find(|w| w[0].gain_db >= 0.0 && w[1].gain_db < 0.0)
        .map(|w| cross(w[0].hz, w[0].gain_db, w[1].hz, w[1].gain_db, 0.0))
}

/// Measure one AC scalar, or refuse and say why.
pub fn measure(pts: &[Point], metric: AcMetric) -> Result<AcMeasurement, Refusal> {
    validate(pts)?;

    let (mut peak_db, mut peak_hz, mut peak_i) = (pts[0].gain_db, pts[0].hz, 0usize);
    for (i, p) in pts.iter().enumerate() {
        if p.gain_db > peak_db {
            peak_db = p.gain_db;
            peak_hz = p.hz;
            peak_i = i;
        }
    }

    let value = match metric {
        AcMetric::Gain => pts[0].gain_db,
        AcMetric::Bandwidth => {
            let target = peak_db - 3.0;
            // Search above the peak: the corner is where the response rolls off, and a response
            // that peaks also crosses the same level on the way up.
            let w = pts[peak_i..]
                .windows(2)
                .find(|w| w[0].gain_db >= target && w[1].gain_db < target)
                .ok_or_else(|| Refusal::NoCrossing {
                    metric: "bandwidth",
                    what: format!("a fall to {target:.3} dB (3 dB below the {peak_db:.3} dB peak)"),
                })?;
            cross(w[0].hz, w[0].gain_db, w[1].hz, w[1].gain_db, target)
        }
        AcMetric::UnityFrequency => unity_hz(pts).ok_or_else(|| Refusal::NoCrossing {
            metric: "unity-frequency",
            what: "a downward 0 dB crossing".into(),
        })?,
        AcMetric::PhaseMargin => {
            let f = unity_hz(pts).ok_or_else(|| Refusal::NoCrossing {
                metric: "phase-margin",
                what: "a downward 0 dB crossing to take the phase at".into(),
            })?;
            180.0 + at(pts, f, |p| p.phase_deg)
        }
    };

    events::ac_measured(metric.as_str(), value, metric.unit(), pts.len());
    Ok(AcMeasurement {
        metric,
        value,
        unit: metric.unit(),
        points: pts.len(),
        peak_db,
        peak_hz,
    })
}
