//! The timer's music, made right here (no files to download, no rights to clear): a few calm
//! pieces synthesised note by note into a loop that joins up without a seam, mono, 22 050 Hz.
//! Notes that ring past the end of the loop wrap round to its start, and the reverb runs twice
//! through it, so the end flows into the beginning exactly as it will when it repeats.

pub const RATE: u32 = 22_050;
const SR: f32 = RATE as f32;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Track {
    Lofi,
    Ambient,
    Piano,
    Rain,
}

/// Every piece: id (as stored on the board), name, a line about it, icon.
pub const TRACKS: [(Track, &str, &str, &str, &str); 4] = [
    (Track::Lofi, "lofi", "Lo-fi", "Accordi caldi e batteria morbida", "headphones"),
    (Track::Ambient, "ambient", "Ambient", "Tappeti lenti per concentrarsi", "cloud"),
    (Track::Piano, "piano", "Pianoforte", "Arpeggi calmi", "music"),
    (Track::Rain, "rain", "Pioggia", "Pioggia leggera e un accordo lontano", "cloud-rain"),
];

impl Track {
    pub fn id(self) -> &'static str {
        TRACKS.iter().find(|t| t.0 == self).map_or("lofi", |t| t.1)
    }
    pub fn from_id(id: &str) -> Option<Track> {
        TRACKS.iter().find(|t| t.1 == id).map(|t| t.0)
    }
}

/* ---------------- building blocks ---------------- */

const TABLE: usize = 4096;

/// One period of a sine, and of a soft saw (its first eight harmonics): oscillators read them
/// instead of calling `sin` millions of times.
struct Tables {
    sine: Vec<f32>,
    saw: Vec<f32>,
}

fn tables() -> &'static Tables {
    static T: std::sync::OnceLock<Tables> = std::sync::OnceLock::new();
    T.get_or_init(|| {
        let at = |i: usize| i as f32 / TABLE as f32 * std::f32::consts::TAU;
        let sine = (0..=TABLE).map(|i| at(i).sin()).collect();
        let saw = (0..=TABLE).map(|i| (1..=8).map(|k| (at(i) * k as f32).sin() / k as f32 * (1.0 - k as f32 / 10.0)).sum::<f32>() * 0.6).collect();
        Tables { sine, saw }
    })
}

/// A table read at `phase` (in cycles, any value).
fn read(t: &[f32], phase: f32) -> f32 {
    let p = phase.rem_euclid(1.0) * TABLE as f32;
    let i = (p as usize).min(TABLE - 1);
    let f = p - i as f32;
    t[i] + (t[i + 1] - t[i]) * f
}

fn sine(phase: f32) -> f32 {
    read(&tables().sine, phase)
}

fn midi(n: f32) -> f32 {
    440.0 * 2f32.powf((n - 69.0) / 12.0)
}

/// A small fixed random generator: the same piece every time.
struct Rng(u32);

impl Rng {
    fn next(&mut self) -> f32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;
        self.0 as f32 / u32::MAX as f32
    }
    fn noise(&mut self) -> f32 {
        self.next() * 2.0 - 1.0
    }
}

/// The loop being written: notes are added at a time in seconds and wrap round its end.
struct Loop {
    buf: Vec<f32>,
}

impl Loop {
    fn new(seconds: f32) -> Loop {
        Loop { buf: vec![0.0; (seconds * SR) as usize] }
    }
    /// Adds `len` seconds of `voice(t)` starting at `at` seconds.
    fn add(&mut self, at: f32, len: f32, mut voice: impl FnMut(f32) -> f32) {
        let n = self.buf.len();
        let start = (at * SR) as usize;
        for i in 0..(len * SR) as usize {
            self.buf[(start + i) % n] += voice(i as f32 / SR);
        }
    }
}

fn attack(t: f32, a: f32) -> f32 {
    if t < a { t / a } else { 1.0 }
}

/// An exponential decay with time constant `tau` seconds, one sample at a time: `next()` is
/// e^(-t/tau) for t = 0, 1/SR, 2/SR… (a multiplication instead of an `exp` per sample).
struct Decay {
    v: f32,
    k: f32,
}

impl Decay {
    fn new(tau: f32) -> Decay {
        Decay { v: 1.0, k: (-1.0 / (tau * SR)).exp() }
    }
    fn next(&mut self) -> f32 {
        let v = self.v;
        self.v *= self.k;
        v
    }
}

/// A Rhodes-like electric piano: a sine bent by another (FM) that mellows as it rings.
fn ep(l: &mut Loop, at: f32, note: f32, hold: f32, gain: f32) {
    let f = midi(note);
    let len = hold + 1.2;
    let (mut body, mut bright, mut tine, mut rel) = (Decay::new(1.6), Decay::new(0.35), Decay::new(0.05), Decay::new(0.28));
    l.add(at, len, |t| {
        let release = if t > hold { rel.next() } else { 1.0 };
        let env = attack(t, 0.004) * body.next() * release;
        let idx = 1.1 * bright.next() + 0.12;
        let ph = f * t;
        let ting = tine.next();
        let tine = if ting > 0.001 { 0.08 * ting * sine(ph * 7.0) } else { 0.0 };
        gain * env * (sine(ph + idx * sine(ph)) + tine)
    });
}

/// A soft piano: slightly stretched partials, the higher ones dying first.
fn piano(l: &mut Loop, at: f32, note: f32, gain: f32) {
    let f = midi(note);
    let damp = (note - 60.0).max(0.0) * 0.02;
    // Each partial: frequency, loudness, and its own decay (the whole note's folded in).
    let mut partials: Vec<(f32, f32, Decay)> = (1..=6)
        .map(|k| {
            let k = k as f32;
            (f * k * (1.0 + 0.0004 * k * k).sqrt(), 1.0 / k.powf(1.3), Decay::new(1.0 / (1.2 + 0.9 * k + damp + 1.0 / 2.6)))
        })
        .collect();
    l.add(at, 3.5, |t| {
        let a = attack(t, 0.003);
        let s: f32 = partials.iter_mut().map(|(pf, amp, d)| {
            let e = d.next();
            if e > 0.002 { *amp * e * sine(*pf * t) } else { 0.0 }
        }).sum();
        gain * a * s
    });
}

fn bass(l: &mut Loop, at: f32, note: f32, hold: f32, gain: f32) {
    let f = midi(note);
    let (mut body, mut rel) = (Decay::new(1.1), Decay::new(0.04));
    l.add(at, hold + 0.12, |t| {
        let env = attack(t, 0.01) * body.next() * if t > hold { rel.next() } else { 1.0 };
        let y = sine(f * t) + 0.25 * sine(2.0 * f * t);
        gain * env * (1.4 * y).tanh()
    });
}

/// A slow pad: three gently detuned soft saws, a little vibrato.
fn pad(l: &mut Loop, at: f32, note: f32, hold: f32, gain: f32) {
    let f = midi(note);
    let saw = &tables().saw;
    let mut rel = Decay::new(0.7);
    l.add(at, hold + 2.0, |t| {
        let env = attack(t, 1.4) * if t > hold { rel.next() } else { 1.0 };
        let vib = 1.0 + 0.0025 * sine(5.0 * t);
        let y = read(saw, f * t * vib) + read(saw, f * 1.004 * t) + read(saw, f * 0.996 * t + 0.3);
        gain * env * y / 3.0
    });
}

fn bell(l: &mut Loop, at: f32, note: f32, gain: f32) {
    let f = midi(note);
    let (mut d1, mut d2, mut d3) = (Decay::new(2.4), Decay::new(0.9), Decay::new(0.4));
    l.add(at, 4.0, |t| {
        let a = attack(t, 0.002);
        gain * a * (d1.next() * sine(f * t) + 0.35 * d2.next() * sine(2.76 * f * t) + 0.15 * d3.next() * sine(5.4 * f * t))
    });
}

fn kick(l: &mut Loop, at: f32, gain: f32) {
    let mut phase = 0.0;
    let (mut pitch, mut body) = (Decay::new(0.035), Decay::new(0.2));
    l.add(at, 0.45, |_| {
        phase += (46.0 + 90.0 * pitch.next()) / SR;
        gain * body.next() * sine(phase)
    });
}

fn snare(l: &mut Loop, at: f32, gain: f32, rng: &mut Rng) {
    let mut lp = 0.0;
    let mut r = Rng((rng.next() * u32::MAX as f32) as u32 | 1);
    let (mut d1, mut d2) = (Decay::new(0.07), Decay::new(0.04));
    l.add(at, 0.3, |t| {
        lp += (r.noise() - lp) * 0.35;
        gain * (0.7 * lp * d1.next() + 0.3 * sine(185.0 * t) * d2.next())
    });
}

fn hat(l: &mut Loop, at: f32, gain: f32, rng: &mut Rng) {
    let mut prev = 0.0;
    let mut r = Rng((rng.next() * u32::MAX as f32) as u32 | 1);
    let mut d = Decay::new(0.022);
    l.add(at, 0.08, |_| {
        let n = r.noise();
        let hp = n - prev;
        prev = n;
        gain * hp * d.next()
    });
}

/* ---------------- effects ---------------- */

/// A small Schroeder reverb (four combs, two all-passes), run twice round the loop so its tail
/// carries over the seam; `wet` 0–1.
fn reverb(buf: &mut [f32], wet: f32, room: f32) {
    let combs = [558usize, 594, 638, 678];
    let alls = [112usize, 170];
    let mut cb: Vec<Vec<f32>> = combs.iter().map(|&d| vec![0.0; (d as f32 * room) as usize]).collect();
    let mut ci = [0usize; 4];
    let mut lp = [0.0f32; 4];
    let mut ab: Vec<Vec<f32>> = alls.iter().map(|&d| vec![0.0; d]).collect();
    let mut ai = [0usize; 2];
    let dry = buf.to_vec();
    for pass in 0..2 {
        for (i, &x) in dry.iter().enumerate() {
            let x = x * 0.3;
            let mut y = 0.0;
            for k in 0..4 {
                let out = cb[k][ci[k]];
                lp[k] += (out - lp[k]) * 0.6;
                cb[k][ci[k]] = x + lp[k] * 0.82;
                ci[k] = (ci[k] + 1) % cb[k].len();
                y += out;
            }
            for k in 0..2 {
                let d = ab[k][ai[k]];
                let v = y + d * 0.5;
                ab[k][ai[k]] = v;
                ai[k] = (ai[k] + 1) % ab[k].len();
                y = d - v * 0.5;
            }
            if pass == 1 {
                buf[i] = dry[i] * (1.0 - wet * 0.5) + y * wet;
            }
        }
    }
}

/// A gentle low-pass (one pole), warmed up on a lap of the loop so it has no seam either.
fn warm(buf: &mut [f32], k: f32) {
    let mut y = 0.0;
    for v in buf.iter() {
        y += (*v - y) * k;
    }
    for v in buf.iter_mut() {
        y += (*v - y) * k;
        *v = y;
    }
}

fn normalize(buf: &mut [f32], peak: f32) {
    let m = buf.iter().fold(0.0f32, |m, v| m.max(v.abs()));
    if m > 0.0 {
        let k = peak / m;
        for v in buf.iter_mut() {
            *v *= k;
        }
    }
}

/* ---------------- the pieces ---------------- */

/// The loop of a piece, ready to play over and over.
pub fn render(track: Track) -> Vec<f32> {
    let mut buf = match track {
        Track::Lofi => lofi(),
        Track::Ambient => ambient(),
        Track::Piano => piano_piece(),
        Track::Rain => rain(),
    };
    normalize(&mut buf, 0.72);
    buf
}

fn lofi() -> Vec<f32> {
    let beat = 60.0 / 76.0;
    let bar = beat * 4.0;
    let bars = 16;
    let mut l = Loop::new(bar * bars as f32);
    let mut rng = Rng(0x1234_5678);
    // ii – V – I – vi in C, rich voicings.
    let chords: [(&[f32], f32); 4] = [(&[53.0, 57.0, 60.0, 64.0], 38.0), (&[53.0, 59.0, 64.0, 69.0], 43.0), (&[52.0, 55.0, 59.0, 62.0], 36.0), (&[55.0, 60.0, 64.0, 71.0], 45.0)];
    let scale = [62.0, 64.0, 67.0, 69.0, 72.0, 74.0, 76.0];
    for b in 0..bars {
        let t0 = b as f32 * bar;
        let (notes, root) = chords[b % 4];
        let next_root = chords[(b + 1) % 4].1;
        // Chord on the one, again (softer) on the and of two: strummed a little.
        for (hit, at, gain) in [(0, 0.0, 0.11), (1, 1.5, 0.07)] {
            for (i, &n) in notes.iter().enumerate() {
                ep(&mut l, t0 + at * beat + i as f32 * 0.012 + hit as f32 * 0.01, n, beat * 1.3, gain);
            }
        }
        bass(&mut l, t0, root, beat * 1.2, 0.32);
        bass(&mut l, t0 + 1.5 * beat, root + 7.0, beat * 0.8, 0.22);
        bass(&mut l, t0 + 3.5 * beat, next_root + if next_root > root { -1.0 } else { 1.0 }, beat * 0.4, 0.18);
        // Drums from the fifth bar: a lazy boom-bap with swung hats.
        if b >= 4 {
            kick(&mut l, t0, 0.5);
            kick(&mut l, t0 + 2.75 * beat, 0.35);
            kick(&mut l, t0 + 3.5 * beat, 0.25);
            snare(&mut l, t0 + beat, 0.22, &mut rng);
            snare(&mut l, t0 + 3.0 * beat, 0.24, &mut rng);
        }
        for e in 0..8 {
            let swing = if e % 2 == 1 { 0.09 } else { 0.0 };
            if rng.next() < 0.88 {
                hat(&mut l, t0 + (e as f32 * 0.5 + swing) * beat, if e % 2 == 0 { 0.07 } else { 0.045 }, &mut rng);
            }
        }
        // A few melody notes in the second half.
        if b >= 8 {
            let mut at = 0.0;
            while at < 4.0 {
                if rng.next() < 0.45 {
                    let n = scale[(rng.next() * scale.len() as f32) as usize % scale.len()];
                    ep(&mut l, t0 + at * beat, n + 12.0, beat * 0.9, 0.06);
                }
                at += if rng.next() < 0.5 { 1.0 } else { 0.5 };
            }
        }
    }
    // Dust on the record: a faint hiss and a few crackles.
    let mut crackle = 0.0f32;
    let mut hiss = 0.0f32;
    for v in l.buf.iter_mut() {
        hiss += (rng.noise() - hiss) * 0.15;
        if rng.next() < 0.0006 {
            crackle = rng.noise() * 0.12;
        }
        crackle *= 0.82;
        *v += hiss * 0.006 + crackle;
    }
    warm(&mut l.buf, 0.42);
    reverb(&mut l.buf, 0.22, 1.0);
    l.buf
}

fn ambient() -> Vec<f32> {
    let span = 8.0;
    let mut l = Loop::new(span * 8.0);
    let mut rng = Rng(0x0bad_cafe);
    let chords: [&[f32]; 4] = [&[48.0, 55.0, 59.0, 62.0, 64.0], &[45.0, 52.0, 55.0, 59.0, 60.0], &[41.0, 53.0, 57.0, 60.0, 64.0], &[43.0, 50.0, 55.0, 57.0, 62.0]];
    for c in 0..8 {
        for &n in chords[c % 4] {
            pad(&mut l, c as f32 * span, n, span, 0.055);
        }
    }
    let penta = [72.0, 74.0, 76.0, 79.0, 81.0, 84.0];
    let mut t = 1.0;
    while t < span * 8.0 {
        let n = penta[(rng.next() * penta.len() as f32) as usize % penta.len()];
        bell(&mut l, t, n, 0.05 + rng.next() * 0.04);
        t += 1.6 + rng.next() * 2.4;
    }
    warm(&mut l.buf, 0.3);
    reverb(&mut l.buf, 0.55, 1.4);
    l.buf
}

fn piano_piece() -> Vec<f32> {
    let beat = 60.0 / 70.0;
    let bar = beat * 4.0;
    let mut l = Loop::new(bar * 16.0);
    let mut rng = Rng(0x00c0_ffee);
    // C – G/B – Am – F, then Dm7 – G – Em – F.
    let prog: [[f32; 4]; 8] = [[48.0, 55.0, 60.0, 64.0], [47.0, 55.0, 59.0, 62.0], [45.0, 52.0, 57.0, 60.0], [41.0, 48.0, 53.0, 57.0], [50.0, 57.0, 60.0, 65.0], [43.0, 50.0, 55.0, 59.0], [40.0, 47.0, 55.0, 59.0], [41.0, 48.0, 53.0, 57.0]];
    for b in 0..16 {
        let t0 = b as f32 * bar;
        let c = prog[b % 8];
        // Left hand: a broken chord in eighths.
        let pattern = [c[0], c[1], c[2], c[3], c[2] + 12.0, c[3], c[2], c[1]];
        for (i, n) in pattern.iter().enumerate() {
            piano(&mut l, t0 + i as f32 * beat * 0.5, *n, if i == 0 { 0.16 } else { 0.1 });
        }
        // Right hand: a simple line on the chord's notes, an octave up.
        if b % 2 == 1 || b >= 8 {
            let tones = [c[1] + 12.0, c[2] + 12.0, c[3] + 12.0, c[2] + 24.0];
            let mut at = 0.0;
            while at < 4.0 {
                let n = tones[(rng.next() * 4.0) as usize % 4];
                piano(&mut l, t0 + at * beat, n, 0.09);
                at += if rng.next() < 0.6 { 1.0 } else { 2.0 };
            }
        }
    }
    reverb(&mut l.buf, 0.3, 1.2);
    l.buf
}

fn rain() -> Vec<f32> {
    let secs = 48.0;
    let mut l = Loop::new(secs);
    let mut rng = Rng(0x5eed_1234);
    // A far, quiet chord underneath.
    for &n in &[36.0, 43.0, 52.0, 55.0] {
        pad(&mut l, 0.0, n, secs, 0.03);
    }
    let n = l.buf.len();
    let (mut a, mut b) = (0.0f32, 0.0f32);
    let mut drop = 0.0f32;
    let mut drop_lp = 0.0f32;
    for i in 0..n {
        let t = i as f32 / SR;
        // Steady rain: noise softened twice, swelling slowly (whole cycles in the loop).
        a += (rng.noise() - a) * 0.5;
        b += (a - b) * 0.25;
        let swell = 0.75 + 0.25 * sine(t * 3.0 / secs) * sine(t * 2.0 / secs + 0.25);
        // Drops: tiny ticks here and there.
        if rng.next() < 0.004 {
            drop = rng.noise() * 0.5;
        }
        drop *= 0.72;
        drop_lp += (drop - drop_lp) * 0.6;
        l.buf[i] += b * 0.22 * swell + (drop - drop_lp) * 0.25;
    }
    reverb(&mut l.buf, 0.25, 1.0);
    l.buf
}

/// The sound of the timer running out: a soft three-note chime.
pub fn chime() -> Vec<f32> {
    let mut l = Loop::new(2.6);
    for (i, n) in [79.0, 83.0, 86.0].iter().enumerate() {
        bell(&mut l, i as f32 * 0.16, *n, 0.25);
    }
    // Not a loop: the wrapped tail is faded out.
    let n = l.buf.len();
    for (i, v) in l.buf.iter_mut().enumerate() {
        *v *= (1.0 - i as f32 / n as f32).powf(0.5);
    }
    normalize(&mut l.buf, 0.6);
    l.buf
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_piece_plays_cleanly_and_loops_without_a_jump() {
        for (track, id, ..) in TRACKS {
            let t = std::time::Instant::now();
            let buf = render(track);
            assert!(buf.len() as f32 / SR > 30.0, "{id}: at least half a minute before it repeats");
            assert!(buf.iter().all(|v| v.is_finite()), "{id}: no broken samples");
            let peak = buf.iter().fold(0.0f32, |m, v| m.max(v.abs()));
            assert!(peak <= 0.75 && peak > 0.5, "{id}: peak {peak}");
            let rms = (buf.iter().map(|v| v * v).sum::<f32>() / buf.len() as f32).sqrt();
            assert!(rms > 0.03, "{id}: audible, rms {rms}");
            // Where it repeats, the last sample leads into the first like any two neighbours.
            let step = (buf[0] - buf[buf.len() - 1]).abs();
            let typical = buf.windows(2).map(|w| (w[1] - w[0]).abs()).fold(0.0f32, f32::max);
            assert!(step <= typical, "{id}: seam {step} vs {typical}");
            assert_eq!(Track::from_id(id), Some(track));
            println!("{id}: {:.1} s in {:?}", buf.len() as f32 / SR, t.elapsed());
        }
        assert!(chime().iter().any(|v| v.abs() > 0.3));
    }
}
