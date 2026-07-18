//! The input records: a time series, or a frequency sweep.
//!
//! Both are small, line-oriented text — deliberately not JSON, so a series can be produced by
//! `awk` from a simulator's output without a serializer in the way, and so a malformed file
//! fails on the line that is wrong rather than at a byte offset.
//!
//! ```text
//! # series.samples  — one value per line, in capture order
//! 0.0
//! 0.7071
//! 1.0
//!
//! # sweep.ac — hz gain_db phase_deg
//! 1.0     40.0   -0.6
//! 10.0    39.9   -5.7
//! ```
//!
//! Blank lines and `#` comments are ignored. A line that is not parseable is an error naming the
//! line number: a silently dropped sample would shift every later one and quietly change the
//! measurement.

use crate::transfer::Point;

/// Read a time series: one finite value per line.
pub fn read_series(text: &str) -> Result<Vec<f64>, String> {
    let mut out = Vec::new();
    for (i, raw) in text.lines().enumerate() {
        let line = raw.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        match line.parse::<f64>() {
            Ok(v) => out.push(v),
            Err(_) => return Err(format!("line {}: {line:?} is not a number", i + 1)),
        }
    }
    if out.is_empty() {
        return Err("no samples found".into());
    }
    Ok(out)
}

/// Read a frequency sweep: `hz gain_db phase_deg` per line.
pub fn read_sweep(text: &str) -> Result<Vec<Point>, String> {
    let mut out = Vec::new();
    for (i, raw) in text.lines().enumerate() {
        let line = raw.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let f: Vec<&str> = line.split_whitespace().collect();
        if f.len() < 3 {
            return Err(format!(
                "line {}: expected `hz gain_db phase_deg`, found {} field(s)",
                i + 1,
                f.len()
            ));
        }
        let num = |s: &str, what: &str| -> Result<f64, String> {
            s.parse::<f64>()
                .map_err(|_| format!("line {}: {what} {s:?} is not a number", i + 1))
        };
        out.push(Point {
            hz: num(f[0], "frequency")?,
            gain_db: num(f[1], "gain")?,
            phase_deg: num(f[2], "phase")?,
        });
    }
    if out.is_empty() {
        return Err("no sweep points found".into());
    }
    Ok(out)
}

/// Parse a comma-separated harmonic-order list, e.g. `2,3,4,5`.
pub fn parse_harmonics(s: &str) -> Result<Vec<usize>, String> {
    s.split(',')
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .map(|p| {
            p.parse::<usize>()
                .map_err(|_| format!("harmonic order {p:?} is not a number"))
        })
        .collect()
}
