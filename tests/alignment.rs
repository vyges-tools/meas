//! The alignment claim must reach the caller, and must never overstate the evidence.
//!
//! The point of the ladder is that a number and a standard's name printed near each other read
//! as a conformance claim whether or not one was meant. These assert that what actually leaves
//! the tool says how much it is claiming — and, in the candidate case, what it is *not*.

use std::process::Command;

fn run(args: &[&str]) -> String {
    let exe = env!("CARGO_BIN_EXE_vyges-meas");
    let out = Command::new(exe)
        .args(args)
        .output()
        .expect("run vyges-meas");
    String::from_utf8_lossy(&out.stdout).to_string()
}

const SERIES: &str = "examples/adc8/capture.samples";

#[test]
fn a_result_never_travels_without_its_claim() {
    for app in ["generic", "adc", "dac", "recorder"] {
        let out = run(&[
            "spectral",
            SERIES,
            "--fundamental-bin",
            "37",
            "--metric",
            "sinad",
            "--application",
            app,
            "--json",
        ]);
        assert!(
            out.contains("\"alignment\""),
            "{app}: every result carries an alignment block"
        );
        assert!(
            out.contains("\"application\": \""),
            "{app}: and names the declared application"
        );
    }
}

#[test]
fn undeclared_application_claims_no_standard() {
    let out = run(&[
        "spectral",
        SERIES,
        "--fundamental-bin",
        "37",
        "--metric",
        "snr",
        "--json",
    ]);
    assert!(
        out.contains("\"level\": \"vyges-definition\""),
        "the default claims nothing"
    );
    assert!(out.contains("\"edition\": null"), "and names no edition");
}

#[test]
fn a_candidate_claim_names_its_edition_and_disclaims_conformance() {
    let cases = [
        ("adc", "IEEE 1241-2023"),
        ("dac", "IEEE 1658-2023"),
        ("recorder", "IEEE 1057-2017"),
    ];
    for (app, edition) in cases {
        let out = run(&[
            "spectral",
            SERIES,
            "--fundamental-bin",
            "37",
            "--metric",
            "sinad",
            "--application",
            app,
            "--json",
        ]);
        assert!(
            out.contains("\"level\": \"candidate\""),
            "{app} reaches candidate"
        );
        assert!(
            out.contains(edition),
            "{app} names {edition} exactly, not the family"
        );
        assert!(
            out.contains("NO clause-level review"),
            "{app}: a candidate claim must say what it is not"
        );
    }
}

/// The ceiling is enforced end to end, not only in the type: nothing the binary can be asked to
/// print says `reviewed` or `conformant`.
#[test]
fn the_binary_cannot_be_made_to_claim_conformance() {
    for app in ["generic", "adc", "dac", "recorder"] {
        for metric in ["snr", "sinad", "thd", "sfdr"] {
            let out = run(&[
                "spectral",
                SERIES,
                "--fundamental-bin",
                "37",
                "--metric",
                metric,
                "--harmonics",
                "2,3",
                "--application",
                app,
                "--json",
            ]);
            assert!(
                !out.contains("\"level\": \"reviewed\"")
                    && !out.contains("\"level\": \"conformant\""),
                "{app}/{metric} claimed a rung no review supports"
            );
        }
    }
}

#[test]
fn the_ac_side_carries_a_claim_too() {
    let out = run(&[
        "transfer",
        "examples/adc8/opamp.ac",
        "--metric",
        "phase-margin",
        "--json",
    ]);
    assert!(
        out.contains("\"alignment\""),
        "AC results carry the claim as well"
    );
    assert!(
        out.contains("vyges-definition"),
        "no standard is claimed for AC transfer"
    );
}

#[test]
fn an_unknown_application_is_rejected_rather_than_defaulted() {
    let exe = env!("CARGO_BIN_EXE_vyges-meas");
    let out = Command::new(exe)
        .args([
            "spectral",
            SERIES,
            "--fundamental-bin",
            "37",
            "--metric",
            "snr",
            "--application",
            "spaceship",
        ])
        .output()
        .expect("run");
    assert_eq!(
        out.status.code(),
        Some(2),
        "a typo must not silently fall back to generic"
    );
}
