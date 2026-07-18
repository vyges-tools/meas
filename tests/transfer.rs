//! The AC kernel against responses whose answer is known in closed form.
//!
//! A single-pole response has textbook values: DC gain `A0`, a −3 dB corner exactly at the pole,
//! unity gain at the gain-bandwidth product `A0·f_p`, and phase approaching −90°. Building the
//! sweep from the transfer function means every expectation here is derived from the maths, not
//! from what the code happened to return.

use vyges_meas::transfer::{measure, AcMetric, Point, Refusal};

/// A single-pole low-pass: `H(f) = A0 / (1 + j f/f_p)`.
fn single_pole(a0_db: f64, fp: f64, decades: i32, per_decade: usize) -> Vec<Point> {
    let a0 = 10f64.powf(a0_db / 20.0);
    let start = (fp.log10() - decades as f64).floor();
    let n = (decades * 2) as usize * per_decade;
    (0..=n)
        .map(|i| {
            let hz = 10f64.powf(start + i as f64 / per_decade as f64);
            let r = hz / fp;
            let mag = a0 / (1.0 + r * r).sqrt();
            Point {
                hz,
                gain_db: 20.0 * mag.log10(),
                phase_deg: -r.atan().to_degrees(),
            }
        })
        .collect()
}

/// `gain` is the value at the lowest swept point, which is not quite DC — three decades below
/// the pole a single-pole response is already 4·10⁻⁶ dB down. The expectation is therefore taken
/// from the transfer function at that same frequency, not from the nominal A0: asserting a round
/// 40 would be asserting something the sweep does not contain.
#[test]
fn gain_is_the_low_frequency_value() {
    let (a0_db, fp) = (40.0, 1.0e3);
    let pts = single_pole(a0_db, fp, 3, 40);
    let m = measure(&pts, AcMetric::Gain).unwrap();
    let r = pts[0].hz / fp;
    let expect = a0_db - 20.0 * (1.0 + r * r).sqrt().log10();
    assert!(
        (m.value - expect).abs() < 1e-12,
        "gain should be {expect} dB, got {}",
        m.value
    );
    assert!(
        (m.value - a0_db).abs() < 1e-4,
        "and within 10⁻⁴ dB of the nominal 40 dB"
    );
}

/// The −3 dB corner of a single pole is the pole frequency itself. A 40-point-per-decade sweep
/// interpolated in (log f, dB) should land within a fraction of a percent.
#[test]
fn bandwidth_finds_the_pole() {
    for fp in [1.0e3, 2.5e4, 1.0e6] {
        let pts = single_pole(40.0, fp, 3, 40);
        let m = measure(&pts, AcMetric::Bandwidth).unwrap();
        let err = (m.value - fp).abs() / fp;
        assert!(
            err < 5e-3,
            "corner should be {fp}, got {} ({:.3}% off)",
            m.value,
            err * 100.0
        );
    }
}

/// Unity gain of a single pole is the gain-bandwidth product: `A0 · f_p`.
#[test]
fn unity_frequency_is_the_gain_bandwidth_product() {
    let (a0_db, fp) = (40.0, 1.0e3);
    let pts = single_pole(a0_db, fp, 4, 60);
    let m = measure(&pts, AcMetric::UnityFrequency).unwrap();
    let gbw = 10f64.powf(a0_db / 20.0) * fp; // 100 × 1 kHz = 100 kHz
    let err = (m.value - gbw).abs() / gbw;
    assert!(
        err < 5e-3,
        "unity should be {gbw} Hz, got {} ({:.3}% off)",
        m.value,
        err * 100.0
    );
}

/// A single pole can only ever reach −90°, so its phase margin tends to 90°. Well above the
/// pole the phase is essentially there.
#[test]
fn a_single_pole_has_about_ninety_degrees_of_margin() {
    let pts = single_pole(40.0, 1.0e3, 4, 60);
    let m = measure(&pts, AcMetric::PhaseMargin).unwrap();
    assert!(
        (m.value - 90.0).abs() < 1.0,
        "single-pole margin ≈ 90°, got {}",
        m.value
    );
}

/// Two poles an octave apart cross unity with appreciably less margin — the metric has to move
/// in the right direction and by a plausible amount, not just return something.
#[test]
fn a_second_pole_eats_phase_margin() {
    let (a0_db, fp1, fp2) = (40.0, 1.0e3, 2.0e4);
    let a0 = 10f64.powf(a0_db / 20.0);
    let pts: Vec<Point> = (0..=300)
        .map(|i| {
            let hz = 10f64.powf(1.0 + i as f64 / 50.0);
            let (r1, r2) = (hz / fp1, hz / fp2);
            let mag = a0 / ((1.0 + r1 * r1).sqrt() * (1.0 + r2 * r2).sqrt());
            Point {
                hz,
                gain_db: 20.0 * mag.log10(),
                phase_deg: -(r1.atan().to_degrees() + r2.atan().to_degrees()),
            }
        })
        .collect();
    let m = measure(&pts, AcMetric::PhaseMargin).unwrap();
    assert!(
        m.value > 0.0 && m.value < 90.0,
        "a second pole must reduce margin, got {}",
        m.value
    );
    // Unity is near 100 kHz, well past the 20 kHz pole, so that pole contributes ~-79°.
    assert!(
        m.value < 30.0,
        "margin should be well under 30° here, got {}",
        m.value
    );
}

/// Bandwidth is referenced to the **peak**, not to the first point. A response that peaks before
/// rolling off would otherwise report a corner that is simply wrong.
#[test]
fn bandwidth_is_referenced_to_the_peak() {
    // Rises 6 dB, then falls away.
    let pts = vec![
        Point {
            hz: 1.0,
            gain_db: 20.0,
            phase_deg: 0.0,
        },
        Point {
            hz: 10.0,
            gain_db: 26.0,
            phase_deg: -10.0,
        },
        Point {
            hz: 100.0,
            gain_db: 23.0,
            phase_deg: -45.0,
        },
        Point {
            hz: 1000.0,
            gain_db: 3.0,
            phase_deg: -90.0,
        },
    ];
    let m = measure(&pts, AcMetric::Bandwidth).unwrap();
    assert!((m.peak_db - 26.0).abs() < 1e-9, "peak is 26 dB");
    // −3 dB from the 26 dB peak is 23 dB, which the sweep reaches exactly at 100 Hz.
    assert!(
        (m.value - 100.0).abs() < 1e-6,
        "corner should be 100 Hz, got {}",
        m.value
    );
}

/// Nothing is extrapolated: a crossing outside the swept range is reported as absent, because a
/// sweep that stopped too early is a fixable mistake and a guessed number is not.
#[test]
fn crossings_outside_the_sweep_are_not_invented() {
    // Never falls to unity.
    let flat = vec![
        Point {
            hz: 1.0,
            gain_db: 20.0,
            phase_deg: 0.0,
        },
        Point {
            hz: 10.0,
            gain_db: 19.9,
            phase_deg: -1.0,
        },
    ];
    assert!(matches!(
        measure(&flat, AcMetric::UnityFrequency),
        Err(Refusal::NoCrossing {
            metric: "unity-frequency",
            ..
        })
    ));
    assert!(matches!(
        measure(&flat, AcMetric::PhaseMargin),
        Err(Refusal::NoCrossing {
            metric: "phase-margin",
            ..
        })
    ));
    assert!(matches!(
        measure(&flat, AcMetric::Bandwidth),
        Err(Refusal::NoCrossing {
            metric: "bandwidth",
            ..
        })
    ));
}

/// An unusable sweep is refused rather than interpolated into nonsense.
#[test]
fn malformed_sweeps_are_refused() {
    let p = |hz: f64| Point {
        hz,
        gain_db: 0.0,
        phase_deg: 0.0,
    };
    assert!(matches!(
        measure(&[p(1.0)], AcMetric::Gain),
        Err(Refusal::TooFewPoints(1))
    ));
    assert!(matches!(
        measure(&[p(10.0), p(1.0)], AcMetric::Gain),
        Err(Refusal::NotMonotonic(1))
    ));
    assert!(matches!(
        measure(&[p(0.0), p(1.0)], AcMetric::Gain),
        Err(Refusal::NotMonotonic(0))
    ));
    let bad = vec![
        p(1.0),
        Point {
            hz: 10.0,
            gain_db: f64::NAN,
            phase_deg: 0.0,
        },
    ];
    assert!(matches!(
        measure(&bad, AcMetric::Gain),
        Err(Refusal::NotFinite(1))
    ));
}
