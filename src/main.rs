//! vyges-meas CLI.
//!
//!   vyges-meas spectral SERIES --fundamental-bin N --metric M [--harmonics 2,3,4,5]
//!   vyges-meas transfer SWEEP  --metric M
//!
//! Exit codes: 0 ok · 1 runtime error · 2 usage · 3 the target was not met
//! (only with `--target`).

use std::process::exit;

use vyges_meas::spectral::{self, Metric, Spec};
use vyges_meas::transfer::{self, AcMetric};
use vyges_meas::{events, job};

const USAGE: &str = "\
vyges-meas — closed measurement kernels (coherent single-tone spectral, AC transfer)

usage:
  vyges-meas spectral SERIES --fundamental-bin N --metric snr|sinad|thd|sfdr
                             [--harmonics 2,3,4,5] [--clip LEVEL] [--target DB]
  vyges-meas transfer SWEEP  --metric gain|bandwidth|unity-frequency|phase-margin
                             [--target VALUE]

SERIES is one sample per line, in capture order. The record must be a power of two
between 8 and 65536 samples and must be COHERENTLY sampled: the fundamental has to
land exactly on a DFT bin, and --fundamental-bin says which. There is no window
function — a rectangular window is exact for a coherent capture, and a non-coherent
one is refused rather than silently smeared.

SWEEP is `hz gain_db phase_deg` per line, strictly increasing in frequency. Values
between points are interpolated in (log10 f, dB); nothing is extrapolated past the
swept range.

Every choice the method makes is documented in `vyges-meas --describe` and in the
module docs. These are Vyges definitions, NOT a claim of IEEE 1241/1658/1057
conformance.

flags:
  --fundamental-bin N   DFT bin the fundamental sits on (spectral; required)
  --metric M            which scalar to measure (required)
  --harmonics LIST      harmonic orders to account for, e.g. 2,3,4,5 (spectral)
  --clip LEVEL          treat |sample| >= LEVEL as clipped and refuse to measure
  --target VALUE        pass/fail threshold; SNR/SINAD/SFDR/gain want >= , THD <=
  -o FILE               write the report to FILE (default: stdout)
  --json                machine-readable JSON instead of the text report
  --describe            print a machine-readable JSON description of the command
  -h, --help · -V, --version
";

const DESCRIBE: &str = r#"{
  "name": "meas",
  "summary": "closed measurement kernels (coherent single-tone spectral, AC transfer)",
  "maturity": "structured",
  "provenance_limitations": [
    "input_hash covers the argument vector, not the content of the series or sweep file it names.",
    "The measurement describes the record it was given; it cannot tell whether that record was captured coherently, and a non-coherent capture is refused rather than detected."
  ],
  "invocation": {
    "args_template": ["spectral", "{series}", "--fundamental-bin", "{fundamental_bin}", "--metric", "{metric}"],
    "optional": [
      { "arg": "harmonics", "flag": "--harmonics" },
      { "arg": "clip", "flag": "--clip" },
      { "arg": "target", "flag": "--target" },
      { "arg": "out", "flag": "-o" }
    ],
    "emits_json": true
  },
  "inputs": {
    "type": "object",
    "required": ["series", "fundamental_bin", "metric"],
    "properties": {
      "series": { "type": "string", "description": "captured time series, one sample per line" },
      "fundamental_bin": { "type": "string", "description": "DFT bin the fundamental sits on" },
      "metric": { "type": "string", "description": "snr | sinad | thd | sfdr" },
      "harmonics": { "type": "string", "description": "harmonic orders to account for, e.g. 2,3,4,5" },
      "clip": { "type": "string", "description": "treat |sample| >= this as clipped and refuse" },
      "target": { "type": "string", "description": "pass/fail threshold in dB" },
      "out": { "type": "string", "description": "write the report to FILE instead of stdout" }
    }
  },
  "artifacts": [ { "role": "measurement_report", "field": "report_path" } ],
  "assertion": {
    "id": "measurement-meets-target",
    "field": "met",
    "pass_when": { "is_true": true }
  },
  "consumes": ["series", "ac_sweep"]
}
"#;

fn opt(args: &[String], name: &str) -> Option<String> {
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1).cloned())
}

fn die(msg: &str) -> ! {
    eprintln!("error: {msg}");
    exit(1)
}

/// Add `report_path` to a `--json` payload so the result says where its report landed.
fn with_report_path(json: &str, path: Option<&str>) -> String {
    let (Some(p), Some(rest)) = (path, json.trim_start().strip_prefix('{')) else {
        return json.to_string();
    };
    let esc = p.replace('\\', "\\\\").replace('"', "\\\"");
    let sep = if rest.trim_start().starts_with('}') {
        ""
    } else {
        ","
    };
    format!("{{\"report_path\": \"{esc}\"{sep}{rest}")
}

/// `-o` writes the report; the machine payload still goes to stdout, so asking for the file does
/// not cost the caller the parsed result.
fn write_out(text: &str, out: Option<&str>, json: bool) {
    match out {
        Some(p) => {
            if let Err(e) = std::fs::write(p, text) {
                die(&format!("{p}: {e}"));
            }
            eprintln!("wrote {p}");
            if json {
                print!("{text}");
            }
        }
        None => print!("{text}"),
    }
}

/// `met` is tri-state. Without a `--target` there is no claim to make — the engine measured
/// something and asserts nothing about it — so the field is `null` and the verdict resolves to
/// `unknown` rather than to a pass nobody asked for.
fn met_json(target: Option<f64>, value: f64, higher_is_better: bool) -> (String, &'static str) {
    match target {
        None => ("null".into(), "unknown"),
        Some(t) => {
            let ok = if higher_is_better {
                value >= t
            } else {
                value <= t
            };
            (ok.to_string(), if ok { "pass" } else { "fail" })
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--describe") {
        print!("{DESCRIBE}");
        return;
    }
    if args.iter().any(|a| a == "-h" || a == "--help") || args.is_empty() {
        print!("{USAGE}");
        return;
    }
    if args.iter().any(|a| a == "-V" || a == "--version") {
        println!("vyges-meas {}", vyges_meas::VERSION);
        println!("{}", vyges_meas::COPYRIGHT);
        return;
    }

    let json = args.iter().any(|a| a == "--json");
    let out = opt(&args, "-o");
    let target = opt(&args, "--target").map(|t| {
        t.parse::<f64>()
            .unwrap_or_else(|_| die(&format!("--target {t:?} is not a number")))
    });
    let metric_s = opt(&args, "--metric").unwrap_or_else(|| {
        eprintln!("error: --metric is required\n{USAGE}");
        exit(2)
    });

    match args[0].as_str() {
        "spectral" => {
            let path = args
                .get(1)
                .filter(|a| !a.starts_with('-'))
                .unwrap_or_else(|| {
                    eprintln!("error: `spectral` needs a SERIES path\n{USAGE}");
                    exit(2)
                });
            let metric = Metric::parse(&metric_s).unwrap_or_else(|| {
                eprintln!("error: unknown --metric {metric_s:?}\n{USAGE}");
                exit(2)
            });
            let bin: usize = opt(&args, "--fundamental-bin")
                .unwrap_or_else(|| {
                    eprintln!("error: --fundamental-bin is required\n{USAGE}");
                    exit(2)
                })
                .parse()
                .unwrap_or_else(|_| die("--fundamental-bin must be a whole number"));
            let harmonics = opt(&args, "--harmonics")
                .map(|s| job::parse_harmonics(&s).unwrap_or_else(|e| die(&e)))
                .unwrap_or_default();
            let clip = opt(&args, "--clip").map(|c| {
                c.parse::<f64>()
                    .unwrap_or_else(|_| die("--clip must be a number"))
            });

            let text =
                std::fs::read_to_string(path).unwrap_or_else(|e| die(&format!("{path}: {e}")));
            let samples = job::read_series(&text).unwrap_or_else(|e| die(&format!("{path}: {e}")));
            let spec = Spec {
                fundamental_bin: bin,
                harmonics,
                clip_level: clip,
                metric,
            };

            match spectral::measure(&samples, &spec) {
                Ok(m) => {
                    for &(order, b) in &m.harmonic_bins {
                        if order * bin > samples.len() / 2 {
                            events::folded(order, b);
                        }
                    }
                    let higher_is_better = metric != Metric::Thd;
                    let (met, verdict) = met_json(target, m.db, higher_is_better);
                    events::done(metric.as_str(), verdict, &format!("{:.4} dB", m.db));
                    let body = if json {
                        with_report_path(&render_json(&m, &met), out.as_deref())
                    } else {
                        render_text(&m, target)
                    };
                    write_out(&body, out.as_deref(), json);
                    if verdict == "fail" && args.iter().any(|a| a == "--fail-on-violation") {
                        exit(3);
                    }
                }
                Err(r) => {
                    // A refusal is a result, not a crash: the input cannot support this method,
                    // and saying so is more useful than a number that would not mean anything.
                    if let spectral::Refusal::Clipped { index, level } = r {
                        events::clipped(index, level);
                    }
                    events::refused(metric.as_str(), &r.to_string());
                    events::done(metric.as_str(), "unknown", "input refused");
                    let body = if json {
                        format!(
                            "{{\n  \"metric\": \"{}\",\n  \"met\": null,\n  \"refused\": {}\n}}\n",
                            metric.as_str(),
                            jstr(&r.to_string())
                        )
                    } else {
                        format!("vyges-meas — {} NOT MEASURED\n  {r}\n", metric.as_str())
                    };
                    write_out(&body, out.as_deref(), json);
                    exit(1);
                }
            }
        }
        "transfer" => {
            let path = args
                .get(1)
                .filter(|a| !a.starts_with('-'))
                .unwrap_or_else(|| {
                    eprintln!("error: `transfer` needs a SWEEP path\n{USAGE}");
                    exit(2)
                });
            let metric = AcMetric::parse(&metric_s).unwrap_or_else(|| {
                eprintln!("error: unknown --metric {metric_s:?}\n{USAGE}");
                exit(2)
            });
            let text =
                std::fs::read_to_string(path).unwrap_or_else(|e| die(&format!("{path}: {e}")));
            let pts = job::read_sweep(&text).unwrap_or_else(|e| die(&format!("{path}: {e}")));

            match transfer::measure(&pts, metric) {
                Ok(m) => {
                    let (met, verdict) = met_json(target, m.value, true);
                    events::done(
                        metric.as_str(),
                        verdict,
                        &format!("{:.6} {}", m.value, m.unit),
                    );
                    let body = if json {
                        with_report_path(&render_ac_json(&m, &met), out.as_deref())
                    } else {
                        render_ac_text(&m, target)
                    };
                    write_out(&body, out.as_deref(), json);
                }
                Err(r) => {
                    events::refused(metric.as_str(), &r.to_string());
                    events::done(metric.as_str(), "unknown", "input refused");
                    let body = if json {
                        format!(
                            "{{\n  \"metric\": \"{}\",\n  \"met\": null,\n  \"refused\": {}\n}}\n",
                            metric.as_str(),
                            jstr(&r.to_string())
                        )
                    } else {
                        format!("vyges-meas — {} NOT MEASURED\n  {r}\n", metric.as_str())
                    };
                    write_out(&body, out.as_deref(), json);
                    exit(1);
                }
            }
        }
        other => {
            eprintln!("error: unknown command {other:?}\n{USAGE}");
            exit(2);
        }
    }
}

fn jstr(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

fn render_json(m: &spectral::Measurement, met: &str) -> String {
    let hb: Vec<String> = m
        .harmonic_bins
        .iter()
        .map(|(o, b)| format!("{{\"order\": {o}, \"bin\": {b}}}"))
        .collect();
    format!(
        "{{\n  \"metric\": {},\n  \"db\": {:.6},\n  \"met\": {},\n  \"n\": {},\n  \
         \"fundamental_bin\": {},\n  \"harmonics\": [{}],\n  \"spur_bin\": {},\n  \
         \"p_f\": {:.12},\n  \"p_h\": {:.12},\n  \"p_n\": {:.12},\n  \"p_r\": {:.12},\n  \
         \"p_s\": {:.12},\n  \"method\": \"vyges-coherent-single-tone/1\"\n}}\n",
        jstr(m.metric.as_str()),
        m.db,
        met,
        m.n,
        m.fundamental_bin,
        hb.join(", "),
        m.spur_bin,
        m.p_f,
        m.p_h,
        m.p_n,
        m.p_r,
        m.p_s
    )
}

fn render_text(m: &spectral::Measurement, target: Option<f64>) -> String {
    let mut s = format!("vyges-meas — {} = {:.4} dB\n", m.metric.as_str(), m.db);
    s.push_str(&format!(
        "  record    {} samples, fundamental on bin {}\n",
        m.n, m.fundamental_bin
    ));
    if !m.harmonic_bins.is_empty() {
        let hb: Vec<String> = m
            .harmonic_bins
            .iter()
            .map(|(o, b)| format!("{o}->bin {b}"))
            .collect();
        s.push_str(&format!("  harmonics {}\n", hb.join(", ")));
    }
    s.push_str(&format!("  worst spur at bin {}\n", m.spur_bin));
    s.push_str(&format!(
        "  power     fundamental {:.6e}  harmonics {:.6e}  noise {:.6e}\n",
        m.p_f, m.p_h, m.p_n
    ));
    if let Some(t) = target {
        let ok = if m.metric == spectral::Metric::Thd {
            m.db <= t
        } else {
            m.db >= t
        };
        s.push_str(&format!(
            "  target    {t:.4} dB -> {}\n",
            if ok { "MET" } else { "NOT MET" }
        ));
    }
    s.push_str("\n  method: vyges-coherent-single-tone/1 (not an IEEE conformance claim)\n");
    s
}

fn render_ac_json(m: &transfer::AcMeasurement, met: &str) -> String {
    format!(
        "{{\n  \"metric\": {},\n  \"value\": {:.9},\n  \"unit\": {},\n  \"met\": {},\n  \
         \"points\": {},\n  \"peak_db\": {:.6},\n  \"peak_hz\": {:.6},\n  \
         \"method\": \"vyges-ac-transfer/1\"\n}}\n",
        jstr(m.metric.as_str()),
        m.value,
        jstr(m.unit),
        met,
        m.points,
        m.peak_db,
        m.peak_hz
    )
}

fn render_ac_text(m: &transfer::AcMeasurement, target: Option<f64>) -> String {
    let mut s = format!(
        "vyges-meas — {} = {:.6} {}\n",
        m.metric.as_str(),
        m.value,
        m.unit
    );
    s.push_str(&format!("  sweep     {} point(s)\n", m.points));
    s.push_str(&format!(
        "  peak gain {:.4} dB at {:.6} Hz\n",
        m.peak_db, m.peak_hz
    ));
    if let Some(t) = target {
        s.push_str(&format!(
            "  target    {t:.6} -> {}\n",
            if m.value >= t { "MET" } else { "NOT MET" }
        ));
    }
    s.push_str("\n  method: vyges-ac-transfer/1\n");
    s
}
