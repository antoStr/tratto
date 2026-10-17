//! The timer's music, made right here (no files to download, no rights to clear): a few calm
//! pieces synthesised note by note into a loop that joins up without a seam, mono, 22 050 Hz.
//! Notes that ring past the end of the loop wrap round to its start, and the reverb runs twice
//! through it, so the end flows into the beginning exactly as it will when it repeats.

pub const RATE: u32 = 22_050;
const SR: f32 = RATE as f32;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Track {
    Acoustic,
    Lofi,
    Smash,
    Friday,
    Synthwave,
    Hero,
}

/// Every piece, as FigJam's: id (as stored on the board), title, the colour of its cover.
pub const TRACKS: [(Track, &str, &str, &str); 6] = [
    (Track::Acoustic, "acoustic", "Acoustic ambient", "#14AE5C"),
    (Track::Lofi, "lofi", "Lo-fi hip hop", "#FFE24B"),
    (Track::Smash, "smash", "Button smash", "#5B5BF6"),
    (Track::Friday, "friday", "It's Friday", "#FF7A1A"),
    (Track::Synthwave, "synthwave", "Synthwave", "#C04BF2"),
    (Track::Hero, "hero", "Hero's journey", "#9BD2FF"),
];

impl Track {
    pub fn id(self) -> &'static str {
        TRACKS.iter().find(|t| t.0 == self).map_or("lofi", |t| t.1)
    }
    pub fn from_id(id: &str) -> Option<Track> {
        // The pieces of Tratto 2.4 played on as the closest new ones.
        let id = match id {
            "ambient" | "piano" => "acoustic",
            "rain" => "lofi",
            other => other,
        };
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
    /// A square of seven odd harmonics: the 8-bit sound.
    square: Vec<f32>,
}

fn tables() -> &'static Tables {
    static T: std::sync::OnceLock<Tables> = std::sync::OnceLock::new();
    T.get_or_init(|| {
        let at = |i: usize| i as f32 / TABLE as f32 * std::f32::consts::TAU;
        let sine = (0..=TABLE).map(|i| at(i).sin()).collect();
        let saw = (0..=TABLE).map(|i| (1..=8).map(|k| (at(i) * k as f32).sin() / k as f32 * (1.0 - k as f32 / 10.0)).sum::<f32>() * 0.6).collect();
        let square = (0..=TABLE).map(|i| (0..7).map(|k| (2 * k + 1) as f32).map(|h| (at(i) * h).sin() / h).sum::<f32>() * 0.9).collect();
        Tables { sine, saw, square }
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

/// A plucked string (Karplus–Strong): a burst of noise in a delay the length of the note,
/// softened on every round: a nylon guitar.
fn pluck(l: &mut Loop, at: f32, note: f32, gain: f32, rng: &mut Rng) {
    let f = midi(note);
    let n = ((SR / f) as usize).max(2);
    let mut line: Vec<f32> = (0..n).map(|_| rng.noise()).collect();
    // A first softening of the burst: a rounder attack.
    for _ in 0..2 {
        for i in 0..n {
            line[i] = 0.5 * (line[i] + line[(i + 1) % n]);
        }
    }
    let mut i = 0;
    let mut fade = Decay::new(2.2);
    l.add(at, 3.2, |_| {
        let j = (i + 1) % n;
        let y = line[i];
        line[i] = 0.4985 * (line[i] + line[j]);
        i = j;
        gain * y * fade.next()
    });
}

/// An 8-bit square note, short and bright, with a quick vibrato after a moment.
fn chip(l: &mut Loop, at: f32, note: f32, len: f32, gain: f32) {
    let f = midi(note);
    let sq = &tables().square;
    let mut rel = Decay::new(0.03);
    l.add(at, len + 0.1, |t| {
        let env = attack(t, 0.003) * if t > len { rel.next() } else { 1.0 } * (1.0 - 0.25 * (t / len.max(0.01)).min(1.0));
        let vib = 1.0 + if t > 0.12 { 0.006 * sine(6.0 * t) } else { 0.0 };
        gain * env * read(sq, f * t * vib)
    });
}

/// A synth voice: soft saws whose brightness opens and closes with each note (`cut` 0–1).
fn synth(l: &mut Loop, at: f32, note: f32, len: f32, gain: f32, cut: f32) {
    let f = midi(note);
    let saw = &tables().saw;
    let (mut env_f, mut rel) = (Decay::new(0.25), Decay::new(0.12));
    let mut lp = 0.0f32;
    l.add(at, len + 0.5, |t| {
        let env = attack(t, 0.008) * if t > len { rel.next() } else { 1.0 };
        let y = 0.5 * (read(saw, f * t) + read(saw, f * 1.006 * t + 0.4));
        let k = (cut * (0.15 + 0.85 * env_f.next())).clamp(0.02, 1.0);
        lp += (y - lp) * k;
        gain * env * lp
    });
}

/// Brass: a saw that swells in and brightens as it does.
fn brass(l: &mut Loop, at: f32, note: f32, len: f32, gain: f32) {
    let f = midi(note);
    let saw = &tables().saw;
    let mut rel = Decay::new(0.2);
    let mut lp = 0.0f32;
    l.add(at, len + 0.8, |t| {
        let swell = attack(t, 0.12);
        let env = swell * if t > len { rel.next() } else { 1.0 };
        let vib = 1.0 + 0.003 * sine(5.2 * t) * attack(t, 0.4);
        let y = read(saw, f * t * vib);
        lp += (y - lp) * (0.05 + 0.25 * swell);
        gain * env * lp * 1.6
    });
}

/// Hand claps: three quick bursts of noise and a short tail.
fn clap(l: &mut Loop, at: f32, gain: f32, rng: &mut Rng) {
    let mut r = Rng((rng.next() * u32::MAX as f32) as u32 | 1);
    let (mut bp1, mut bp2) = (0.0f32, 0.0f32);
    let mut tail = Decay::new(0.06);
    l.add(at, 0.3, |t| {
        let n = r.noise();
        bp1 += (n - bp1) * 0.45;
        bp2 += (bp1 - bp2) * 0.45;
        let band = bp1 - bp2;
        let burst = [0.0, 0.011, 0.022].iter().map(|&s| if t >= s && t < s + 0.009 { 1.0 } else { 0.0 }).sum::<f32>();
        let env = if t < 0.03 { burst } else { tail.next() * 0.8 };
        gain * env * band * 2.5
    });
}

/// An open hi-hat: like a closed one, ringing longer.
fn open_hat(l: &mut Loop, at: f32, gain: f32, rng: &mut Rng) {
    let mut prev = 0.0;
    let mut r = Rng((rng.next() * u32::MAX as f32) as u32 | 1);
    let mut d = Decay::new(0.12);
    l.add(at, 0.4, |_| {
        let n = r.noise();
        let hp = n - prev;
        prev = n;
        gain * hp * d.next()
    });
}

/// A timpani: a low drum with a pitch, and a soft skin noise.
fn timpani(l: &mut Loop, at: f32, note: f32, gain: f32, rng: &mut Rng) {
    let f = midi(note);
    let mut r = Rng((rng.next() * u32::MAX as f32) as u32 | 1);
    let (mut body, mut skin) = (Decay::new(0.9), Decay::new(0.05));
    let mut lp = 0.0f32;
    l.add(at, 2.4, |t| {
        lp += (r.noise() - lp) * 0.08;
        gain * (body.next() * (sine(f * t) + 0.3 * sine(f * 1.5 * t)) + 0.6 * skin.next() * lp)
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
        Track::Acoustic => acoustic(),
        Track::Lofi => lofi(),
        Track::Smash => smash(),
        Track::Friday => friday(),
        Track::Synthwave => synthwave(),
        Track::Hero => hero(),
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

/// Acoustic ambient: a nylon guitar picking through open chords, a soft pad and a few bells.
fn acoustic() -> Vec<f32> {
    let beat = 60.0 / 84.0;
    let bar = beat * 4.0;
    let mut l = Loop::new(bar * 16.0);
    let mut rng = Rng(0x0a0c_0057);
    // G – D/F# – Em – C, open voicings.
    let chords: [[f32; 6]; 4] = [[43.0, 50.0, 55.0, 59.0, 62.0, 67.0], [42.0, 50.0, 54.0, 57.0, 62.0, 66.0], [40.0, 47.0, 52.0, 55.0, 59.0, 64.0], [36.0, 48.0, 52.0, 55.0, 60.0, 64.0]];
    let order = [0usize, 3, 2, 4, 1, 4, 2, 5];
    for b in 0..16 {
        let t0 = b as f32 * bar;
        let c = chords[b % 4];
        for (k, &s) in order.iter().enumerate() {
            pluck(&mut l, t0 + k as f32 * beat * 0.5 + rng.next() * 0.012, c[s], if k == 0 { 0.32 } else { 0.22 }, &mut rng);
        }
        for &n in &c[1..4] {
            pad(&mut l, t0, n + 12.0, bar, 0.018);
        }
        if b % 2 == 1 {
            bell(&mut l, t0 + 2.0 * beat, c[5] + 12.0, 0.035);
        }
    }
    warm(&mut l.buf, 0.5);
    reverb(&mut l.buf, 0.35, 1.3);
    l.buf
}

/// Button smash: a fast 8-bit tune, square-wave arpeggios over a bouncing bass and noise drums.
fn smash() -> Vec<f32> {
    let beat = 60.0 / 150.0;
    let bar = beat * 4.0;
    let mut l = Loop::new(bar * 24.0);
    let mut rng = Rng(0x0b17_0b17);
    // Am – F – C – G, quick arpeggios a step at a time.
    let chords: [[f32; 3]; 4] = [[69.0, 72.0, 76.0], [65.0, 69.0, 72.0], [67.0, 72.0, 76.0], [67.0, 71.0, 74.0]];
    let roots = [45.0, 41.0, 48.0, 43.0];
    let tune = [76.0, 79.0, 81.0, 79.0, 76.0, 74.0, 72.0, 74.0, 76.0, 76.0, 72.0, 69.0, 71.0, 72.0, 74.0, 71.0];
    for b in 0..24 {
        let t0 = b as f32 * bar;
        let c = chords[b % 4];
        for k in 0..16 {
            chip(&mut l, t0 + k as f32 * beat * 0.25, c[k % 3] - 12.0, beat * 0.2, 0.05);
        }
        for k in 0..8 {
            chip(&mut l, t0 + k as f32 * beat * 0.5, roots[b % 4] - if k % 2 == 1 { 0.0 } else { 12.0 }, beat * 0.4, 0.09);
        }
        if b >= 8 {
            let start = (b % 4) * 4;
            for k in 0..4 {
                chip(&mut l, t0 + k as f32 * beat, tune[start + k], beat * 0.85, 0.07);
            }
        }
        if b >= 4 {
            for k in 0..4 {
                kick(&mut l, t0 + k as f32 * beat, if k % 2 == 0 { 0.4 } else { 0.0 });
                if k % 2 == 1 {
                    snare(&mut l, t0 + k as f32 * beat, 0.2, &mut rng);
                }
                hat(&mut l, t0 + (k as f32 + 0.5) * beat, 0.05, &mut rng);
            }
        }
    }
    reverb(&mut l.buf, 0.1, 0.8);
    l.buf
}

/// It's Friday: a disco groove, four on the floor, claps, an octave bass and piano stabs.
fn friday() -> Vec<f32> {
    let beat = 60.0 / 118.0;
    let bar = beat * 4.0;
    let mut l = Loop::new(bar * 24.0);
    let mut rng = Rng(0x0f71_da15);
    // Dm9 – G13 – Cmaj9 – A7: funky and bright.
    let chords: [(&[f32], f32); 4] = [(&[62.0, 65.0, 69.0, 72.0, 76.0], 38.0), (&[59.0, 64.0, 65.0, 69.0], 43.0), (&[60.0, 64.0, 67.0, 71.0, 74.0], 36.0), (&[61.0, 64.0, 67.0, 69.0], 45.0)];
    for b in 0..24 {
        let t0 = b as f32 * bar;
        let (notes, root) = chords[b % 4];
        for k in 0..8 {
            let n = if k % 2 == 0 { root } else { root + 12.0 };
            bass(&mut l, t0 + k as f32 * beat * 0.5, n, beat * 0.35, 0.3);
        }
        for at in [0.5f32, 1.75, 2.5, 3.5] {
            for (i, &n) in notes.iter().enumerate() {
                ep(&mut l, t0 + at * beat + i as f32 * 0.004, n, beat * 0.25, 0.06);
            }
        }
        if b >= 2 {
            for k in 0..4 {
                kick(&mut l, t0 + k as f32 * beat, 0.5);
                open_hat(&mut l, t0 + (k as f32 + 0.5) * beat, 0.07, &mut rng);
                if k % 2 == 1 {
                    clap(&mut l, t0 + k as f32 * beat, 0.25, &mut rng);
                }
            }
        }
        if b >= 8 && b % 2 == 0 {
            for (k, n) in [74.0, 76.0, 77.0, 76.0].iter().enumerate() {
                synth(&mut l, t0 + (2.0 + k as f32 * 0.5) * beat, *n, beat * 0.4, 0.06, 0.35);
            }
        }
    }
    reverb(&mut l.buf, 0.18, 1.0);
    l.buf
}

/// Synthwave: a driving saw bass in eighths, a wide pad, a bright arpeggio and a big snare.
fn synthwave() -> Vec<f32> {
    let beat = 60.0 / 96.0;
    let bar = beat * 4.0;
    let mut l = Loop::new(bar * 16.0);
    let mut rng = Rng(0x05e7_5e75);
    // Am – F – C – G.
    let chords: [([f32; 3], f32); 4] = [([57.0, 60.0, 64.0], 33.0), ([57.0, 60.0, 65.0], 29.0), ([55.0, 60.0, 64.0], 36.0), ([55.0, 59.0, 62.0], 31.0)];
    for b in 0..16 {
        let t0 = b as f32 * bar;
        let (c, root) = chords[b % 4];
        for k in 0..8 {
            synth(&mut l, t0 + k as f32 * beat * 0.5, root + 12.0, beat * 0.4, 0.2, 0.18);
        }
        for &n in &c {
            pad(&mut l, t0, n, bar, 0.04);
        }
        if b >= 4 {
            for k in 0..16 {
                synth(&mut l, t0 + k as f32 * beat * 0.25, c[k % 3] + 12.0 + if k % 6 >= 3 { 12.0 } else { 0.0 }, beat * 0.2, 0.05, 0.5);
            }
            for k in 0..4 {
                kick(&mut l, t0 + k as f32 * beat, 0.5);
                hat(&mut l, t0 + (k as f32 + 0.5) * beat, 0.05, &mut rng);
                if k % 2 == 1 {
                    snare(&mut l, t0 + k as f32 * beat, 0.32, &mut rng);
                }
            }
        }
    }
    reverb(&mut l.buf, 0.32, 1.4);
    l.buf
}

/// Hero's journey: strings, a brass call over them and timpani rolls, rising.
fn hero() -> Vec<f32> {
    let beat = 60.0 / 80.0;
    let bar = beat * 4.0;
    let mut l = Loop::new(bar * 16.0);
    let mut rng = Rng(0x0e70_0e70);
    // D – A – Bm – G, then D – G – A – D.
    let prog: [([f32; 4], f32); 8] = [([50.0, 57.0, 62.0, 66.0], 38.0), ([49.0, 57.0, 61.0, 64.0], 45.0), ([47.0, 54.0, 59.0, 62.0], 35.0), ([43.0, 55.0, 59.0, 62.0], 31.0), ([50.0, 57.0, 62.0, 66.0], 38.0), ([43.0, 55.0, 59.0, 62.0], 31.0), ([45.0, 57.0, 61.0, 64.0], 33.0), ([50.0, 57.0, 62.0, 66.0], 38.0)];
    let call = [[74.0, 0.0, 1.5], [76.0, 1.5, 0.5], [78.0, 2.0, 2.0], [81.0, 0.0, 3.0], [79.0, 3.0, 1.0], [78.0, 0.0, 4.0]];
    for b in 0..16 {
        let t0 = b as f32 * bar;
        let (c, root) = prog[b % 8];
        for &n in &c {
            pad(&mut l, t0, n, bar, 0.045);
        }
        bass(&mut l, t0, root, bar * 0.9, 0.22);
        timpani(&mut l, t0, root + 12.0, 0.3, &mut rng);
        if b % 4 == 3 {
            for k in 0..6 {
                timpani(&mut l, t0 + 3.0 * beat + k as f32 * beat / 6.0, root + 12.0, 0.08 + k as f32 * 0.03, &mut rng);
            }
        }
        if b >= 4 {
            let phrase = (b % 2) * 3;
            for k in 0..3 {
                let [n, at, len] = call[phrase + k];
                brass(&mut l, t0 + at * beat, n - 12.0, len * beat, 0.1);
            }
        }
    }
    warm(&mut l.buf, 0.55);
    reverb(&mut l.buf, 0.42, 1.5);
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
