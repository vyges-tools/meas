//! How strongly a measurement claims alignment with an external standard.
//!
//! A number and a standard's name printed near each other is read as a conformance claim, whether
//! or not one was meant. "THD per IEEE 519" on a converter datasheet is the classic example — 519
//! governs harmonic control in *power systems at a point of common coupling* and has nothing to
//! say about an amplifier or a converter, but the citation looks authoritative and travels.
//!
//! So every result states, in machine-readable form, exactly how much it is claiming:
//!
//! | level | means |
//! |---|---|
//! | `vyges-definition` | the method is ours, complete and versioned. **No external standard is claimed.** |
//! | `candidate` | the application lies inside a named standard's *published scope*, but no clause-level review has been done |
//! | `reviewed` | a crosswalk records the exact edition, clauses, choices, deviations, reviewer and review artifact |
//! | `conformant` | an independently reviewed profile **and** a conformance suite |
//!
//! # The ladder is enforced, not just documented
//!
//! `Reviewed` and `Conformant` can only be built from a [`Crosswalk`], and a `Crosswalk` cannot
//! be constructed without every field of the evidence it represents. There is no way to write a
//! stronger claim than the evidence supports, because there is no constructor for one.
//!
//! **No measurement in this crate is `reviewed` or `conformant` today**, and none can become so by
//! editing a label — it takes a real clause-level review, recorded. That is the point: a
//! discipline kept only in prose is a discipline that quietly lapses.

/// The evidence behind a `reviewed` or `conformant` claim.
///
/// Every field is required. A crosswalk missing its reviewer, or its list of deviations, is not a
/// review — it is an assertion, and asserting is what the ladder exists to prevent.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Crosswalk {
    /// The exact edition reviewed against, e.g. `"IEEE 1241-2023"`. Not the standard family:
    /// clauses move between editions, so a claim against "IEEE 1241" alone is unfalsifiable.
    pub edition: &'static str,
    /// The specific clauses covered.
    pub clauses: &'static [&'static str],
    /// Choices this method makes where the standard permits several.
    pub choices: &'static [&'static str],
    /// Where this method knowingly departs from the standard. An empty list is a claim in itself
    /// and must be deliberate, not the default from nobody having looked.
    pub deviations: &'static [&'static str],
    /// Who performed the review.
    pub reviewer: &'static str,
    /// Where the review artifact lives.
    pub artifact: &'static str,
}

/// The alignment claim attached to a measurement.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Alignment {
    /// Ours, complete, versioned. No external standard is claimed.
    VygesDefinition,
    /// Within a named standard's published scope; no clause-level review.
    Candidate {
        /// The exact edition whose scope this falls inside.
        edition: &'static str,
        /// What that edition's *published scope* says it covers — the basis for the claim, in the
        /// standard's own terms rather than ours.
        scope: &'static str,
    },
    /// A clause-level review has been performed and recorded.
    Reviewed(Crosswalk),
    /// Independently reviewed, with a conformance suite.
    Conformant {
        crosswalk: Crosswalk,
        suite: &'static str,
    },
}

impl Alignment {
    pub fn level(&self) -> &'static str {
        match self {
            Alignment::VygesDefinition => "vyges-definition",
            Alignment::Candidate { .. } => "candidate",
            Alignment::Reviewed(_) => "reviewed",
            Alignment::Conformant { .. } => "conformant",
        }
    }

    /// The edition this claim names, if any.
    pub fn edition(&self) -> Option<&'static str> {
        match self {
            Alignment::VygesDefinition => None,
            Alignment::Candidate { edition, .. } => Some(edition),
            Alignment::Reviewed(c) => Some(c.edition),
            Alignment::Conformant { crosswalk, .. } => Some(crosswalk.edition),
        }
    }

    /// One line stating exactly what is and is not being claimed, for the text report.
    pub fn statement(&self) -> String {
        match self {
            Alignment::VygesDefinition => {
                "vyges-definition — this method is ours; no external standard is claimed".into()
            }
            Alignment::Candidate { edition, scope } => format!(
                "candidate ({edition}) — the application lies within that edition's published \
                 scope ({scope}); NO clause-level review has been performed, so this is not a \
                 conformance claim"
            ),
            Alignment::Reviewed(c) => {
                format!(
                    "reviewed ({}) — crosswalk by {} at {}",
                    c.edition, c.reviewer, c.artifact
                )
            }
            Alignment::Conformant { crosswalk, suite } => {
                format!("conformant ({}) — suite {suite}", crosswalk.edition)
            }
        }
    }
}

/// What the record is a measurement *of*. The application is what decides which standard's scope
/// the result falls inside — the same arithmetic on the same samples is a candidate against a
/// different edition depending on whether it describes a converter or an oscilloscope.
///
/// It has to be declared, and defaults to `Generic`: the tool cannot infer from a list of numbers
/// what device produced them, and guessing would manufacture a standards claim out of nothing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Application {
    /// A sampled waveform of unstated origin.
    Generic,
    /// An analog-to-digital converter.
    Adc,
    /// A digital-to-analog converter.
    Dac,
    /// A digitizing waveform recorder, analyzer or oscilloscope.
    Recorder,
}

impl Application {
    pub fn parse(s: &str) -> Option<Application> {
        match s.to_ascii_lowercase().as_str() {
            "generic" => Some(Application::Generic),
            "adc" => Some(Application::Adc),
            "dac" => Some(Application::Dac),
            "recorder" | "scope" => Some(Application::Recorder),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Application::Generic => "generic",
            Application::Adc => "adc",
            Application::Dac => "dac",
            Application::Recorder => "recorder",
        }
    }

    /// The strongest claim this application supports today.
    ///
    /// The scope strings are taken from each edition's *published scope* — the part that can be
    /// read without a licence. That is deliberately the limit of what we assert: the normative
    /// clauses needed for a conformance claim are not public, so the honest ceiling for anything
    /// here is `candidate`.
    pub fn alignment(self) -> Alignment {
        match self {
            Application::Generic => Alignment::VygesDefinition,
            Application::Adc => Alignment::Candidate {
                edition: "IEEE 1241-2023",
                scope: "terminology and test methods for nominally uniformly sampled and \
                        quantized analog-to-digital converters",
            },
            Application::Dac => Alignment::Candidate {
                edition: "IEEE 1658-2023",
                scope: "terminology and test methods for monolithic, hybrid and module \
                        digital-to-analog converters, not encompassing systems",
            },
            Application::Recorder => Alignment::Candidate {
                edition: "IEEE 1057-2017",
                scope: "terminology and test methods for digitizing waveform recorders",
            },
        }
    }
}

/// Standards that are **not** authority for these measurements, and are cited as if they were.
///
/// IEEE 519 is the one that keeps appearing on converter and amplifier datasheets. It governs
/// harmonic control in electric power systems at a point of common coupling — a completely
/// different quantity, measured at a different place, for a different purpose. A THD figure from
/// this crate has nothing to do with it, and must never be reported as 519-anything.
pub const NOT_AUTHORITY: &[(&str, &str)] = &[(
    "IEEE 519-2022",
    "harmonic control in electric power systems at a point of common coupling — not an authority \
     for amplifier or converter THD, and must not be cited as one",
)];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_application_claims_nothing_external() {
        assert_eq!(Application::Generic.alignment(), Alignment::VygesDefinition);
        assert_eq!(Application::Generic.alignment().edition(), None);
        assert!(Application::Generic
            .alignment()
            .statement()
            .contains("no external standard"));
    }

    #[test]
    fn declaring_an_application_reaches_candidate_and_stops_there() {
        for (app, edition) in [
            (Application::Adc, "IEEE 1241-2023"),
            (Application::Dac, "IEEE 1658-2023"),
            (Application::Recorder, "IEEE 1057-2017"),
        ] {
            let a = app.alignment();
            assert_eq!(a.level(), "candidate", "{app:?} may claim candidate");
            assert_eq!(
                a.edition(),
                Some(edition),
                "{app:?} names its exact edition"
            );
            // The statement has to say what it is NOT, not only what it is.
            assert!(
                a.statement().contains("NO clause-level review"),
                "a candidate claim must disclaim conformance: {}",
                a.statement()
            );
        }
    }

    /// The ladder's top two rungs are unreachable today, and cannot be reached by editing a
    /// label — this asserts that nothing in the crate has quietly climbed them.
    #[test]
    fn nothing_claims_reviewed_or_conformant() {
        for app in [
            Application::Generic,
            Application::Adc,
            Application::Dac,
            Application::Recorder,
        ] {
            let level = app.alignment().level();
            assert!(
                level == "vyges-definition" || level == "candidate",
                "{app:?} claims {level}, but no clause-level review has been performed"
            );
        }
    }

    /// A crosswalk cannot be half-built: every field of the evidence is required, so a `reviewed`
    /// claim carries its own audit trail or does not exist.
    #[test]
    fn a_reviewed_claim_must_carry_its_evidence() {
        let c = Crosswalk {
            edition: "IEEE 1241-2023",
            clauses: &["4.4.3"],
            choices: &["rectangular window; coherent capture"],
            deviations: &["zero-bin integration width"],
            reviewer: "example",
            artifact: "example://review",
        };
        let a = Alignment::Reviewed(c);
        assert_eq!(a.level(), "reviewed");
        assert_eq!(a.edition(), Some("IEEE 1241-2023"));
        // Constructing this required naming a reviewer, an artifact, the clauses covered and the
        // deviations. There is no shorter path to the word "reviewed".
    }

    #[test]
    fn ieee_519_is_recorded_as_not_being_authority() {
        let (name, why) = NOT_AUTHORITY[0];
        assert_eq!(name, "IEEE 519-2022");
        assert!(why.contains("not an authority"));
    }
}
