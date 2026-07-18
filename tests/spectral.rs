//! The spectral kernel against signals whose answer is known in closed form.
//!
//! A measurement kernel that only ever runs on real data can be plausibly wrong forever. These
//! build the signal from the answer: a tone plus a harmonic at a chosen amplitude ratio has a
//! THD that is *exactly* 20·log10(ratio), so the test asserts a number derived independently of
//! the code rather than a number the code produced yesterday.

use std::f64::consts::PI;
use vyges_meas::spectral::{measure, Metric, Refusal, Spec};

const N: usize = 1024;

/// A coherent tone: `amp · sin(2π·bin·t/n)`. Coherent by construction — an integer number of
/// cycles in the record — which is what lets a rectangular window be exact.
fn tone(n: usize, bin: usize, amp: f64) -> Vec<f64> {
    (0..n)
        .map(|t| amp * (2.0 * PI * bin as f64 * t as f64 / n as f64).sin())
        .collect()
}

fn add(a: &mut [f64], b: &[f64]) {
    for (x, y) in a.iter_mut().zip(b) {
        *x += y;
    }
}

fn spec(metric: Metric, bin: usize, harmonics: &[usize]) -> Spec {
    Spec {
        fundamental_bin: bin,
        harmonics: harmonics.to_vec(),
        clip_level: None,
        metric,
    }
}

/// THD is fixed by the amplitude ratio alone: a third harmonic at 1% of the fundamental is
/// −40 dB, at 10% is −20 dB, whatever the record length or bin.
#[test]
fn thd_equals_the_amplitude_ratio_in_db() {
    for (ratio, expect_db) in [(0.01, -40.0), (0.1, -20.0), (0.001, -60.0)] {
        let mut x = tone(N, 5, 1.0);
        add(&mut x, &tone(N, 15, ratio)); // 3rd harmonic of bin 5
        let m = measure(&x, &spec(Metric::Thd, 5, &[3])).expect("measurable");
        assert!(
            (m.db - expect_db).abs() < 1e-9,
            "third harmonic at {ratio} must give {expect_db} dB, got {}",
            m.db
        );
    }
}

/// Several harmonics add in power, not amplitude: two harmonics each at 1% give
/// 10·log10(2·1e-4) = −37.0 dB, not −40.
#[test]
fn harmonics_sum_in_power() {
    let mut x = tone(N, 5, 1.0);
    add(&mut x, &tone(N, 10, 0.01)); // 2nd
    add(&mut x, &tone(N, 15, 0.01)); // 3rd
    let m = measure(&x, &spec(Metric::Thd, 5, &[2, 3])).expect("measurable");
    let expect = 10.0 * (2.0f64 * 1e-4).log10();
    assert!(
        (m.db - expect).abs() < 1e-9,
        "expected {expect} dB, got {}",
        m.db
    );
}

/// SFDR is the distance to the single worst residual bin, whether or not it is a harmonic.
#[test]
fn sfdr_measures_the_worst_single_spur() {
    let mut x = tone(N, 7, 1.0);
    add(&mut x, &tone(N, 23, 0.02)); // a non-harmonic spur, 2% -> -34 dB
    add(&mut x, &tone(N, 14, 0.005)); // a smaller 2nd harmonic
    let m = measure(&x, &spec(Metric::Sfdr, 7, &[2])).expect("measurable");
    assert_eq!(m.spur_bin, 23, "the largest residual bin is the spur");
    // Exactly 20·log10(1/0.02) = 33.9794…, not a rounded 34.
    let expect = 20.0 * (1.0f64 / 0.02).log10();
    assert!(
        (m.db - expect).abs() < 1e-9,
        "SFDR should be {expect} dB, got {}",
        m.db
    );
}

/// With no noise, everything not the fundamental is harmonic — so SINAD is exactly −THD.
/// Two definitions, one signal: they have to agree.
#[test]
fn sinad_is_the_negative_of_thd_when_only_harmonics_are_present() {
    let mut x = tone(N, 9, 1.0);
    add(&mut x, &tone(N, 18, 0.03));
    add(&mut x, &tone(N, 27, 0.02));
    let thd = measure(&x, &spec(Metric::Thd, 9, &[2, 3])).unwrap().db;
    let sinad = measure(&x, &spec(Metric::Sinad, 9, &[2, 3])).unwrap().db;
    assert!(
        (sinad + thd).abs() < 1e-9,
        "SINAD {sinad} should be -THD {thd}"
    );
}

/// SNR excludes the declared harmonics; SINAD does not. Declaring the harmonic must therefore
/// raise SNR above SINAD by exactly the harmonic's contribution.
#[test]
fn snr_excludes_declared_harmonics_and_sinad_does_not() {
    let mut x = tone(N, 11, 1.0);
    add(&mut x, &tone(N, 33, 0.05)); // 3rd harmonic at 5%
    let snr = measure(&x, &spec(Metric::Snr, 11, &[3])).unwrap().db;
    let sinad = measure(&x, &spec(Metric::Sinad, 11, &[3])).unwrap().db;
    assert!(
        snr > sinad,
        "excluding the harmonic must improve SNR: {snr} vs {sinad}"
    );
    // With no other content, removing the only harmonic leaves nothing: SNR is unbounded.
    assert!(
        snr > 250.0,
        "a harmonics-only signal has no noise left, got {snr}"
    );
    assert!(
        (sinad - 26.020_599_913_28).abs() < 1e-6,
        "SINAD = -20log10(0.05), got {sinad}"
    );
}

/// A harmonic above Nyquist does not disappear — it aliases back, and its power is really there.
/// Bin 20 of a 64-sample record: the 3rd harmonic is bin 60, which folds to 64 − 60 = 4.
#[test]
fn harmonics_above_nyquist_fold_back() {
    let n = 64;
    let mut x = tone(n, 20, 1.0);
    add(&mut x, &tone(n, 4, 0.01)); // where the 3rd harmonic actually lands
    let m = measure(&x, &spec(Metric::Thd, 20, &[3])).expect("measurable");
    assert_eq!(
        m.harmonic_bins,
        vec![(3, 4)],
        "3rd harmonic of bin 20 folds to bin 4"
    );
    assert!(
        (m.db - -40.0).abs() < 1e-9,
        "the folded harmonic is measured, got {}",
        m.db
    );
}

/// A pure tone has no distortion at all. THD is then −inf, which is the honest answer rather
/// than a small number that would invite comparison.
#[test]
fn a_pure_tone_has_no_distortion() {
    let x = tone(N, 13, 1.0);
    let m = measure(&x, &spec(Metric::Thd, 13, &[2, 3])).expect("measurable");
    assert!(
        m.db < -250.0 || m.db.is_infinite(),
        "a pure tone's THD is -inf, got {}",
        m.db
    );
}

/// A DC offset must not appear as signal, noise or distortion: it is removed, and the DC bin is
/// excluded from every partition.
#[test]
fn a_dc_offset_changes_nothing() {
    // Give the signal real distortion and a real spur, so each metric has actual content. A
    // pure tone's SNR is limited only by floating-point rounding (~266 dB here), and comparing
    // two such values would be measuring the arithmetic rather than the method.
    let mut clean = tone(N, 17, 1.0);
    add(&mut clean, &tone(N, 34, 0.02));
    add(&mut clean, &tone(N, 51, 0.01));
    add(&mut clean, &tone(N, 200, 0.003));
    let offset: Vec<f64> = clean.iter().map(|v| v + 3.7).collect();
    for metric in [Metric::Snr, Metric::Sinad, Metric::Sfdr, Metric::Thd] {
        let a = measure(&clean, &spec(metric, 17, &[2, 3])).unwrap().db;
        let b = measure(&offset, &spec(metric, 17, &[2, 3])).unwrap().db;
        assert!(
            (a - b).abs() < 1e-9,
            "{metric:?}: DC offset shifted it {a} -> {b}"
        );
    }
}

/// Every refusal is a case where returning a number would be worse than returning nothing.
#[test]
fn unmeasurable_inputs_are_refused_not_guessed() {
    let x = tone(N, 5, 1.0);

    // not a power of two
    let short = tone(N, 5, 1.0)[..1000].to_vec();
    assert!(matches!(
        measure(&short, &spec(Metric::Snr, 5, &[])),
        Err(Refusal::RecordLength(1000))
    ));

    // fundamental outside 1..n/2
    assert!(matches!(
        measure(&x, &spec(Metric::Snr, 0, &[])),
        Err(Refusal::FundamentalBin { bin: 0, .. })
    ));
    assert!(matches!(
        measure(&x, &spec(Metric::Snr, N, &[])),
        Err(Refusal::FundamentalBin { .. })
    ));

    // a clipped capture describes the acquisition, not the device
    let mut clipped = x.clone();
    clipped[42] = 1.5;
    let s = Spec {
        clip_level: Some(1.2),
        ..spec(Metric::Snr, 5, &[])
    };
    assert!(matches!(
        measure(&clipped, &s),
        Err(Refusal::Clipped { index: 42, .. })
    ));

    // a non-finite sample
    let mut nan = x.clone();
    nan[7] = f64::NAN;
    assert!(matches!(
        measure(&nan, &spec(Metric::Snr, 5, &[])),
        Err(Refusal::NotFinite(7))
    ));

    // order 1 is the fundamental
    assert!(matches!(
        measure(&x, &spec(Metric::Snr, 5, &[1])),
        Err(Refusal::HarmonicOrder(1))
    ));
}

/// Collisions are refused rather than double-counted. In a 64-sample record with the fundamental
/// on bin 16, the 2nd harmonic lands on 32 and the 4th folds back onto 0 (DC), while in an
/// 8-bin-fundamental case two orders can fold onto one bin.
#[test]
fn colliding_harmonics_are_refused() {
    let n = 64;
    let x = tone(n, 16, 1.0);

    // 4th harmonic of bin 16 = 64 -> folds to 0 = DC
    assert!(matches!(
        measure(&x, &spec(Metric::Thd, 16, &[4])),
        Err(Refusal::Collision {
            order: 4,
            bin: 0,
            with: "DC"
        })
    ));

    // 3rd (48 -> 16) lands back on the fundamental
    assert!(matches!(
        measure(&x, &spec(Metric::Thd, 16, &[3])),
        Err(Refusal::Collision {
            order: 3,
            with: "the fundamental",
            ..
        })
    ));

    // 2nd (32) and 6th (96 -> 32) land on each other
    assert!(matches!(
        measure(&x, &spec(Metric::Thd, 16, &[2, 6])),
        Err(Refusal::Collision {
            order: 6,
            bin: 32,
            with: "another harmonic"
        })
    ));
}

/// The partitions must account for all the power: residual = harmonics + noise, exactly.
#[test]
fn the_partitions_are_consistent() {
    let mut x = tone(N, 19, 1.0);
    add(&mut x, &tone(N, 38, 0.02));
    add(&mut x, &tone(N, 57, 0.01));
    add(&mut x, &tone(N, 100, 0.004)); // a non-harmonic spur -> noise
    let m = measure(&x, &spec(Metric::Sinad, 19, &[2, 3])).unwrap();
    assert!(
        (m.p_r - (m.p_h + m.p_n)).abs() < 1e-15,
        "residual must split into harmonics + noise"
    );
    assert!(
        m.p_s <= m.p_r,
        "the worst single bin cannot exceed the whole residual"
    );
    // Amplitudes are recoverable: p = A²/2.
    assert!(
        (m.p_f - 0.5).abs() < 1e-12,
        "unit tone has power 0.5, got {}",
        m.p_f
    );
}
