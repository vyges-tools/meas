//! Coherent single-tone spectral metrics — **SNR, SINAD, THD, SFDR**.
//!
//! `SNR` or `THD` on their own are not measurements. The same converter, the same capture, and
//! two honest tools can disagree by several dB purely on convention: whether the fundamental bin
//! is excluded from noise, how many harmonics are counted, whether DC is in the band, where the
//! band ends, what happens to a harmonic that aliases onto another. So this module does not
//! implement "SNR" — it implements *one closed method*, states every choice, and refuses inputs
//! it cannot measure that way.
//!
//! # The method, in full
//!
//! - one finite real time series of uniformly spaced samples;
//! - a **power-of-two** record length between 8 and 65,536;
//! - **coherent** sampling: the fundamental must land exactly on a DFT bin, and the caller says
//!   which. There is no window function and no leakage correction — a rectangular window is
//!   exact for a coherent capture and wrong for anything else, so a non-coherent capture is
//!   rejected rather than silently smeared;
//! - the arithmetic mean is removed, and the DC bin is excluded from every partition;
//! - a one-sided DFT expressed as **mean-square power per bin**;
//! - harmonic orders are named by the caller and **folded into the first Nyquist zone**;
//! - **zero-bin integration width**: each component is exactly one bin, never a skirt;
//! - a closed analysis band, ending at Nyquist;
//! - a harmonic that folds onto the fundamental, onto DC, or onto another harmonic is a
//!   **collision** and is rejected — counting one bin as two components would double-count its
//!   power;
//! - a clipped record is reported as `not_assessed` rather than measured.
//!
//! # The partitions
//!
//! With DC and the fundamental removed from the retained band:
//!
//! | symbol | contents |
//! |---|---|
//! | `p_f` | the fundamental bin |
//! | `p_h` | the declared, folded, non-colliding, in-band harmonic bins |
//! | `p_n` | everything remaining once the harmonic bins are also removed |
//! | `p_r` | all residual bins — harmonics, noise and spurs together |
//! | `p_s` | the largest single residual bin (lowest frequency wins an exact tie) |
//!
//! ```text
//! SNR   = 10 log10(p_f / p_n)      noise only, harmonics excluded
//! SINAD = 10 log10(p_f / p_r)      everything that is not the fundamental
//! THD   = 10 log10(p_h / p_f)      harmonics relative to the fundamental (negative dB)
//! SFDR  = 10 log10(p_f / p_s)      distance to the worst single spur
//! ```
//!
//! # What this is not
//!
//! Not an IEEE-conformant measurement. IEEE 1241 (ADCs), 1658 (DACs) and 1057 (waveform
//! recorders) are the applicable references, and their clause-level requirements have not been
//! reviewed against this code. It is a Vyges definition: complete, reproducible, and honest
//! about which it is. Do not label a result as conforming to a standard it has not been checked
//! against.

use crate::events;

/// Which scalar to compute. One per call, on purpose: a caller that wants four numbers should
/// ask four times and get four independently checked answers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Metric {
    Snr,
    Sinad,
    Thd,
    Sfdr,
}

impl Metric {
    pub fn parse(s: &str) -> Option<Metric> {
        match s.to_ascii_lowercase().as_str() {
            "snr" => Some(Metric::Snr),
            "sinad" => Some(Metric::Sinad),
            "thd" => Some(Metric::Thd),
            "sfdr" => Some(Metric::Sfdr),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Metric::Snr => "snr",
            Metric::Sinad => "sinad",
            Metric::Thd => "thd",
            Metric::Sfdr => "sfdr",
        }
    }
}

/// Everything the method needs beyond the samples themselves.
#[derive(Clone, Debug)]
pub struct Spec {
    /// The DFT bin the fundamental sits on. `1..n/2`; the caller establishes coherence, because
    /// only the caller knows the generator and the capture.
    pub fundamental_bin: usize,
    /// Harmonic orders to account for, e.g. `[2, 3, 4, 5]`. Order 1 is the fundamental and is
    /// rejected here.
    pub harmonics: Vec<usize>,
    /// Samples at or beyond this magnitude are treated as clipped. `None` disables the check.
    pub clip_level: Option<f64>,
    pub metric: Metric,
}

/// A refusal to measure, with the reason. Every one of these is a case where returning a number
/// would be worse than returning nothing.
#[derive(Clone, Debug, PartialEq)]
pub enum Refusal {
    /// Record length is not a power of two in `8..=65536`.
    RecordLength(usize),
    /// A sample is NaN or infinite.
    NotFinite(usize),
    /// The record reaches the declared clip level — the tone is distorted by the capture, not by
    /// the device, so the numbers would describe the wrong thing.
    Clipped { index: usize, level: f64 },
    /// The fundamental bin is outside `1..n/2`.
    FundamentalBin { bin: usize, n: usize },
    /// A harmonic order collides with DC, the fundamental, or another harmonic once folded.
    Collision {
        order: usize,
        bin: usize,
        with: &'static str,
    },
    /// A harmonic order of 0 or 1 was named.
    HarmonicOrder(usize),
    /// The fundamental bin holds no power, so every ratio against it is undefined.
    SilentFundamental,
}

impl std::fmt::Display for Refusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Refusal::RecordLength(n) => write!(
                f,
                "record length {n} is not a power of two in 8..=65536 — a coherent DFT needs one"
            ),
            Refusal::NotFinite(i) => write!(f, "sample {i} is not finite"),
            Refusal::Clipped { index, level } => write!(
                f,
                "sample {index} reaches the clip level {level} — the capture is clipped, so the \
                 tone is distorted by the acquisition rather than by the device under test"
            ),
            Refusal::FundamentalBin { bin, n } => {
                write!(
                    f,
                    "fundamental bin {bin} is outside 1..{} for a {n}-sample record",
                    n / 2
                )
            }
            Refusal::Collision { order, bin, with } => write!(
                f,
                "harmonic {order} folds onto bin {bin}, which is already {with} — counting one \
                 bin as two components would double-count its power"
            ),
            Refusal::HarmonicOrder(o) => {
                write!(
                    f,
                    "harmonic order {o} is not measurable (1 is the fundamental)"
                )
            }
            Refusal::SilentFundamental => {
                write!(
                    f,
                    "the fundamental bin holds no power — every ratio against it is undefined"
                )
            }
        }
    }
}

/// One measured scalar, with the evidence behind it.
#[derive(Clone, Debug)]
pub struct Measurement {
    pub metric: Metric,
    /// The value in dB.
    pub db: f64,
    pub n: usize,
    pub fundamental_bin: usize,
    /// Harmonic order → the bin it folded to.
    pub harmonic_bins: Vec<(usize, usize)>,
    pub p_f: f64,
    pub p_h: f64,
    pub p_n: f64,
    pub p_r: f64,
    /// Worst single residual bin, and where it is — the spur SFDR is measured against.
    pub p_s: f64,
    pub spur_bin: usize,
}

/// Fold a harmonic into the first Nyquist zone.
///
/// A harmonic above Nyquist does not vanish; it aliases back onto a lower bin, and its power is
/// genuinely there in the record. Ignoring that would flatter every high-order measurement, so
/// the fold is part of the method rather than a correction applied afterwards.
fn fold(order: usize, fundamental_bin: usize, n: usize) -> usize {
    let half = n / 2;
    let period = n; // the spectrum repeats every n bins
    let mut b = (order * fundamental_bin) % period;
    if b > half {
        b = period - b; // reflect about Nyquist
    }
    b
}

/// One-sided mean-square power per bin, DC included at index 0.
///
/// A direct DFT, not an FFT: at n ≤ 65,536 the O(n²) cost is irrelevant next to being able to
/// read the definition off the code, and there is no third-party numerics in the trust path.
fn power_spectrum(x: &[f64]) -> Vec<f64> {
    let n = x.len();
    let half = n / 2;
    let mut p = vec![0.0; half + 1];
    for (k, slot) in p.iter_mut().enumerate() {
        let (mut re, mut im) = (0.0, 0.0);
        for (t, &v) in x.iter().enumerate() {
            let ang = -2.0 * std::f64::consts::PI * (k as f64) * (t as f64) / (n as f64);
            re += v * ang.cos();
            im += v * ang.sin();
        }
        let mag2 = (re * re + im * im) / ((n * n) as f64);
        // One-sided: every bin except DC and Nyquist carries its mirror's power too.
        *slot = if k == 0 || (n.is_multiple_of(2) && k == half) {
            mag2
        } else {
            2.0 * mag2
        };
    }
    p
}

/// Measure one scalar from one record, or refuse and say why.
pub fn measure(samples: &[f64], spec: &Spec) -> Result<Measurement, Refusal> {
    let n = samples.len();
    if !(8..=65_536).contains(&n) || !n.is_power_of_two() {
        return Err(Refusal::RecordLength(n));
    }
    for (i, &v) in samples.iter().enumerate() {
        if !v.is_finite() {
            return Err(Refusal::NotFinite(i));
        }
        if let Some(c) = spec.clip_level {
            if v.abs() >= c.abs() {
                return Err(Refusal::Clipped { index: i, level: c });
            }
        }
    }
    let half = n / 2;
    if spec.fundamental_bin == 0 || spec.fundamental_bin > half {
        return Err(Refusal::FundamentalBin {
            bin: spec.fundamental_bin,
            n,
        });
    }

    // Remove the arithmetic mean. DC is excluded from every partition anyway; removing it first
    // keeps the DC bin from carrying a rounding residue into the spectrum.
    let mean = samples.iter().sum::<f64>() / n as f64;
    let centred: Vec<f64> = samples.iter().map(|v| v - mean).collect();
    let p = power_spectrum(&centred);

    // Fold each harmonic and reject collisions. Order matters: a harmonic that lands on an
    // earlier one is a collision, so they are resolved in the order the caller named them.
    let mut harmonic_bins: Vec<(usize, usize)> = Vec::new();
    for &order in &spec.harmonics {
        if order < 2 {
            return Err(Refusal::HarmonicOrder(order));
        }
        let b = fold(order, spec.fundamental_bin, n);
        let with = if b == 0 {
            Some("DC")
        } else if b == spec.fundamental_bin {
            Some("the fundamental")
        } else if harmonic_bins.iter().any(|&(_, hb)| hb == b) {
            Some("another harmonic")
        } else {
            None
        };
        if let Some(with) = with {
            return Err(Refusal::Collision {
                order,
                bin: b,
                with,
            });
        }
        harmonic_bins.push((order, b));
    }

    let p_f = p[spec.fundamental_bin];
    if p_f <= 0.0 {
        return Err(Refusal::SilentFundamental);
    }
    let p_h: f64 = harmonic_bins.iter().map(|&(_, b)| p[b]).sum();

    // Residual = the retained band minus DC and the fundamental. Noise = residual minus the
    // declared harmonics.
    let mut p_r = 0.0;
    let mut p_s = 0.0;
    let mut spur_bin = 0;
    for (k, &pk) in p.iter().enumerate() {
        if k == 0 || k == spec.fundamental_bin {
            continue;
        }
        p_r += pk;
        // Strictly greater: the lowest-frequency bin wins an exact tie, so the answer does not
        // depend on iteration order.
        if pk > p_s {
            p_s = pk;
            spur_bin = k;
        }
    }
    let p_n = (p_r - p_h).max(0.0);

    let db = |num: f64, den: f64| 10.0 * (num / den).log10();
    let value = match spec.metric {
        Metric::Snr => db(p_f, p_n),
        Metric::Sinad => db(p_f, p_r),
        Metric::Thd => db(p_h, p_f),
        Metric::Sfdr => db(p_f, p_s),
    };

    events::measured(spec.metric.as_str(), value, n, spec.fundamental_bin);
    Ok(Measurement {
        metric: spec.metric,
        db: value,
        n,
        fundamental_bin: spec.fundamental_bin,
        harmonic_bins,
        p_f,
        p_h,
        p_n,
        p_r,
        p_s,
        spur_bin,
    })
}
