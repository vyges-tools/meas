# vyges-meas

**Closed measurement kernels for analog characterization** — coherent single-tone spectral
metrics (SNR, SINAD, THD, SFDR) and AC transfer metrics (gain, bandwidth, unity-gain frequency,
phase margin).

## Why the definitions are the product

`SNR` names a ratio, not a measurement. Whether the fundamental is excluded from noise, how many
harmonics are counted, whether DC is in the band, where the band ends, what happens to a harmonic
that aliases back below Nyquist — every one of those changes the number, and two honest tools can
differ by several dB while both being "right". A datasheet figure and a simulation are comparable
only if both state the same choices.

So each kernel fixes **one** method, documents every choice, and **refuses inputs it cannot
measure that way**. A refusal is a result: it says the number would have been meaningless.

## Use

```sh
vyges-meas spectral capture.samples --fundamental-bin 37 --metric thd --harmonics 2,3,4,5
vyges-meas transfer opamp.ac --metric phase-margin --target 45
```

`SERIES` is one sample per line in capture order; `SWEEP` is `hz gain_db phase_deg` per line.
Both accept `#` comments. Add `--json` for machine output, `-o FILE` for a report (the JSON still
goes to stdout, so asking for the file never costs you the parsed result).

## The spectral method

- a power-of-two record, 8 to 65,536 samples;
- **coherent** sampling — the fundamental lands exactly on a DFT bin and the caller says which.
  No window function: a rectangular window is exact for a coherent capture and wrong for anything
  else, so a non-coherent capture is refused rather than silently smeared;
- the arithmetic mean is removed and DC is excluded from every partition;
- harmonics are folded into the first Nyquist zone — an aliased harmonic's power is genuinely in
  the record, and ignoring it would flatter every high-order result;
- **zero-bin integration width**: each component is exactly one bin, never a skirt;
- a harmonic colliding with DC, the fundamental, or another harmonic is **refused**, because
  counting one bin as two components would double-count its power;
- a clipped record is refused: it describes the acquisition, not the device.

```text
SNR   = 10 log10(p_f / p_n)      noise only, declared harmonics excluded
SINAD = 10 log10(p_f / p_r)      everything that is not the fundamental
THD   = 10 log10(p_h / p_f)      harmonics against the fundamental (negative dB)
SFDR  = 10 log10(p_f / p_s)      distance to the worst single spur
```

## The AC method

Values between swept points are interpolated in (log10 f, dB) and (log10 f, degrees). **Nothing
is extrapolated** — a crossing outside the swept range is reported as absent, because a sweep
that stopped too early is a fixable mistake and a guessed number is not.

Bandwidth is referenced to the **peak** gain, not the first point: a response that peaks before
rolling off has its −3 dB corner relative to that peak, and referencing the first point would be
wrong for exactly the circuits where the number matters.

## Verification

Every metric is tested against signals whose answer is known in closed form — a tone plus a
harmonic at a chosen amplitude ratio has a THD that is *exactly* 20·log10(ratio); a single-pole
response has its −3 dB corner *exactly* at the pole and unity gain *exactly* at `A0·f_p`. The
tests assert values derived from the mathematics, not values the code produced previously.

## Not a standards claim

IEEE 1241 (ADCs), 1658 (DACs) and 1057 (waveform recorders) are the applicable references. Their
clause-level requirements have **not** been reviewed against this code, so no result here may be
labelled as conforming to them. These are Vyges definitions — complete, reproducible, and
explicit about which they are.

## License

Apache-2.0. See [LICENSE](LICENSE) and [NOTICE](NOTICE).
