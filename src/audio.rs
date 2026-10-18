//! Playing the timer's music and its chime. Everyone in a board hears the same piece at the
//! same point: where to be is worked out from when it started, on the clock shared with the host.
//! On a computer a small mixer feeds the sound card (ALSA, loaded only when needed, on Linux;
//! cpal on Windows and Mac); in the browser, Web Audio. No sound card, or no sound library:
//! the music is simply silent, nothing else changes.

use crate::music::{self, Track};

/// What should be heard now, called every frame: the piece (and when it started, shared clock,
/// ms) or none, and its volume 0–1 for this person only.
pub fn music(want: Option<(Track, f64)>, volume: f32) {
    imp::music(want, volume.clamp(0.0, 1.0));
}

/// The timer ran out.
pub fn chime(sound: crate::prefs::TimerSound, volume: f32) {
    imp::chime(sound, volume.clamp(0.0, 1.0));
}

/// Where a loop of `len` samples is, `elapsed_ms` after it started.
fn position(elapsed_ms: f64, len: usize) -> f64 {
    (elapsed_ms.max(0.0) / 1000.0 * music::RATE as f64).rem_euclid(len.max(1) as f64)
}

#[cfg(not(target_arch = "wasm32"))]
mod imp {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex};
    use std::time::Instant;

    use super::*;

    #[derive(Default)]
    struct Mixer {
        /// The piece wanted, when it started (shared clock), and its samples once rendered.
        track: Option<(Track, f64)>,
        data: Option<Arc<Vec<f32>>>,
        /// Read position in `data` (samples at `music::RATE`); None: to be placed from the clock.
        pos: Option<f64>,
        volume: f32,
        /// What is heard: follows `volume` (and silence between pieces) without clicks.
        gain: f32,
        chime: Option<(Arc<Vec<f32>>, f64, f32)>,
        quiet_since: Option<Instant>,
    }

    static MIXER: std::sync::LazyLock<Mutex<Mixer>> = std::sync::LazyLock::new(Default::default);
    static RUNNING: AtomicBool = AtomicBool::new(false);

    fn lock() -> std::sync::MutexGuard<'static, Mixer> {
        MIXER.lock().unwrap_or_else(|p| p.into_inner())
    }

    pub fn music(want: Option<(Track, f64)>, volume: f32) {
        let mut m = lock();
        m.volume = volume;
        let changed = m.track.map(|t| t.0) != want.map(|t| t.0);
        let restarted = m.track.zip(want).is_some_and(|(a, b)| a.0 == b.0 && (a.1 - b.1).abs() > 1.0);
        if changed || restarted {
            m.track = want;
            m.pos = None;
            if changed {
                m.data = None;
                if let Some((t, _)) = want {
                    std::thread::spawn(move || {
                        let samples = Arc::new(music::render(t));
                        let mut m = lock();
                        if m.track.is_some_and(|w| w.0 == t) {
                            m.data = Some(samples);
                            m.pos = None;
                        }
                    });
                }
            }
        }
        let wanted = m.track.is_some();
        drop(m);
        if wanted {
            start();
        }
    }

    pub fn chime(sound: crate::prefs::TimerSound, volume: f32) {
        let samples = Arc::new(music::chime_sound(sound));
        lock().chime = Some((samples, 0.0, volume));
        start();
    }

    /// Fills interleaved stereo `out` at `rate` Hz; false once nothing has played for a while
    /// (the sound card is then let go).
    fn fill(out: &mut [f32], rate: u32) -> bool {
        let mut m = lock();
        let step = music::RATE as f64 / rate as f64;
        // The music where it should be by the shared clock: placed when it starts, set right
        // if it drifted by more than a fifth of a second.
        if let (Some((_, at)), Some(data)) = (m.track, m.data.clone()) {
            let len = data.len() as f64;
            let should = position(crate::platform::shared_now() - at, data.len());
            let off = m.pos.map_or(f64::INFINITY, |p| {
                let d = (p - should).rem_euclid(len);
                d.min(len - d)
            });
            if off > 0.2 * music::RATE as f64 {
                m.pos = Some(should);
            }
        }
        let target = if m.track.is_some() && m.data.is_some() { m.volume } else { 0.0 };
        // About 60 ms to reach the target volume.
        let ease = 1.0 - (-1.0 / (0.06 * rate as f32)).exp();
        for frame in out.chunks_mut(2) {
            m.gain += (target - m.gain) * ease;
            let mut s = 0.0;
            if let (Some(data), Some(p)) = (m.data.clone(), m.pos) {
                let len = data.len();
                let i = p as usize % len;
                let f = (p - p.floor()) as f32;
                // Loudness feels right on a square curve: half the slider is a quarter of the power.
                s = (data[i] + (data[(i + 1) % len] - data[i]) * f) * m.gain * m.gain;
                m.pos = Some((p + step) % len as f64);
            }
            if let Some((c, p, v)) = &mut m.chime {
                let i = *p as usize;
                if i + 1 < c.len() {
                    s += c[i] * *v;
                    *p += step;
                } else {
                    m.chime = None;
                }
            }
            let s = s.clamp(-1.0, 1.0);
            frame[0] = s;
            if frame.len() > 1 {
                frame[1] = s;
            }
        }
        let busy = m.track.is_some() || m.chime.is_some() || m.gain > 0.001;
        if busy {
            m.quiet_since = None;
        } else if m.quiet_since.get_or_insert_with(Instant::now).elapsed().as_secs() >= 3 {
            m.quiet_since = None;
            return false;
        }
        true
    }

    #[cfg(test)]
    pub fn running() -> bool {
        RUNNING.load(Ordering::SeqCst)
    }

    /// Opens the sound card on a thread of its own, unless it is already playing.
    fn start() {
        if RUNNING.swap(true, Ordering::SeqCst) {
            return;
        }
        let spawned = std::thread::Builder::new().name("tratto-audio".into()).spawn(|| {
            if let Err(e) = output() {
                eprintln!("audio: {e}");
                lock().chime = None;
            }
            RUNNING.store(false, Ordering::SeqCst);
        });
        if spawned.is_err() {
            RUNNING.store(false, Ordering::SeqCst);
        }
    }

    /// Linux: ALSA's library, opened now (no build dependency; PipeWire and PulseAudio play
    /// what is sent to its "default" device).
    #[cfg(target_os = "linux")]
    fn output() -> Result<(), String> {
        use std::ffi::{c_char, c_int, c_long, c_uint, c_ulong, c_void};
        type Open = unsafe extern "C" fn(*mut *mut c_void, *const c_char, c_int, c_int) -> c_int;
        type SetParams = unsafe extern "C" fn(*mut c_void, c_int, c_int, c_uint, c_uint, c_int, c_uint) -> c_int;
        type Writei = unsafe extern "C" fn(*mut c_void, *const c_void, c_ulong) -> c_long;
        type Recover = unsafe extern "C" fn(*mut c_void, c_int, c_int) -> c_int;
        type Close = unsafe extern "C" fn(*mut c_void) -> c_int;
        const PLAYBACK: c_int = 0;
        const FLOAT_LE: c_int = 14;
        const RW_INTERLEAVED: c_int = 3;
        const RATE: u32 = 48_000;
        const FRAMES: usize = 1024;
        // SAFETY: libasound.so.2 is the system's ALSA library; its five functions are called with
        // the signatures of its stable C API, on a handle used only on this thread and closed here.
        unsafe {
            let lib = libloading::Library::new("libasound.so.2").map_err(|e| e.to_string())?;
            let open: libloading::Symbol<Open> = lib.get(b"snd_pcm_open\0").map_err(|e| e.to_string())?;
            let set_params: libloading::Symbol<SetParams> = lib.get(b"snd_pcm_set_params\0").map_err(|e| e.to_string())?;
            let writei: libloading::Symbol<Writei> = lib.get(b"snd_pcm_writei\0").map_err(|e| e.to_string())?;
            let recover: libloading::Symbol<Recover> = lib.get(b"snd_pcm_recover\0").map_err(|e| e.to_string())?;
            let close: libloading::Symbol<Close> = lib.get(b"snd_pcm_close\0").map_err(|e| e.to_string())?;
            let mut pcm: *mut c_void = std::ptr::null_mut();
            if open(&mut pcm, c"default".as_ptr(), PLAYBACK, 0) < 0 || pcm.is_null() {
                return Err("no sound device".into());
            }
            // Two channels, resampled by ALSA if the card wants another rate, ~120 ms of buffer.
            if set_params(pcm, FLOAT_LE, RW_INTERLEAVED, 2, RATE, 1, 120_000) < 0 {
                close(pcm);
                return Err("the sound device refused the format".into());
            }
            let mut buf = vec![0.0f32; FRAMES * 2];
            'play: while fill(&mut buf, RATE) {
                let mut done = 0usize;
                while done < FRAMES {
                    let n = writei(pcm, buf.as_ptr().add(done * 2) as *const c_void, (FRAMES - done) as c_ulong);
                    if n < 0 {
                        if recover(pcm, n as c_int, 1) < 0 {
                            break 'play;
                        }
                        continue;
                    }
                    done += n as usize;
                }
            }
            close(pcm);
        }
        Ok(())
    }

    /// Windows and Mac: cpal, which needs nothing installed there.
    #[cfg(any(windows, target_os = "macos"))]
    fn output() -> Result<(), String> {
        use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
        let device = cpal::default_host().default_output_device().ok_or("no sound device")?;
        let config = device.default_output_config().map_err(|e| e.to_string())?;
        let format = config.sample_format();
        let config: cpal::StreamConfig = config.into();
        let (rate, channels) = (config.sample_rate, (config.channels as usize).max(1));
        let alive = Arc::new(AtomicBool::new(true));
        let err = |e: cpal::Error| eprintln!("audio: {e}");
        macro_rules! stream {
            ($t:ty) => {{
                let alive = alive.clone();
                let mut stereo: Vec<f32> = Vec::new();
                device.build_output_stream(
                    config.clone(),
                    move |out: &mut [$t], _: &cpal::OutputCallbackInfo| {
                        let frames = out.len() / channels;
                        stereo.resize(frames * 2, 0.0);
                        if !fill(&mut stereo, rate) {
                            alive.store(false, Ordering::SeqCst);
                        }
                        for (i, frame) in out.chunks_mut(channels).enumerate() {
                            for (c, v) in frame.iter_mut().enumerate() {
                                *v = <$t as cpal::FromSample<f32>>::from_sample_(stereo[i * 2 + c.min(1)]);
                            }
                        }
                    },
                    err,
                    None,
                )
            }};
        }
        let stream = match format {
            cpal::SampleFormat::F32 => stream!(f32),
            cpal::SampleFormat::I16 => stream!(i16),
            cpal::SampleFormat::U16 => stream!(u16),
            cpal::SampleFormat::I32 => stream!(i32),
            other => return Err(format!("unsupported sample format {other}")),
        }
        .map_err(|e| e.to_string())?;
        stream.play().map_err(|e| e.to_string())?;
        while alive.load(Ordering::SeqCst) {
            std::thread::sleep(std::time::Duration::from_millis(200));
        }
        Ok(())
    }

    #[cfg(not(any(target_os = "linux", windows, target_os = "macos")))]
    fn output() -> Result<(), String> {
        Err("no audio on this system".into())
    }
}

#[cfg(target_arch = "wasm32")]
mod imp {
    use std::cell::RefCell;

    use super::*;

    struct Web {
        ctx: web_sys::AudioContext,
        gain: web_sys::GainNode,
        /// The piece playing (and when it started), its source node.
        playing: Option<((Track, f64), web_sys::AudioBufferSourceNode)>,
        /// Rendered pieces, kept: switching back costs nothing.
        cache: Vec<(Track, web_sys::AudioBuffer)>,
        chime: Option<web_sys::AudioBuffer>,
    }

    thread_local! {
        static WEB: RefCell<Option<Web>> = const { RefCell::new(None) };
    }

    fn with<R>(f: impl FnOnce(&mut Web) -> R) -> Option<R> {
        WEB.with(|w| {
            let mut w = w.borrow_mut();
            if w.is_none() {
                let ctx = web_sys::AudioContext::new().ok()?;
                let gain = ctx.create_gain().ok()?;
                gain.connect_with_audio_node(&ctx.destination()).ok()?;
                *w = Some(Web { ctx, gain, playing: None, cache: Vec::new(), chime: None });
            }
            w.as_mut().map(f)
        })
    }

    fn buffer(ctx: &web_sys::AudioContext, samples: &[f32]) -> Option<web_sys::AudioBuffer> {
        let b = ctx.create_buffer(1, samples.len() as u32, music::RATE as f32).ok()?;
        b.copy_to_channel(samples, 0).ok()?;
        Some(b)
    }

    pub fn music(want: Option<(Track, f64)>, volume: f32) {
        // Nothing wanted and nothing set up: no audio context for nothing.
        if want.is_none() && WEB.with(|w| w.borrow().is_none()) {
            return;
        }
        with(|w| {
            // Browsers keep audio suspended until the person has interacted with the page.
            if w.ctx.state() == web_sys::AudioContextState::Suspended {
                let _ = w.ctx.resume();
            }
            w.gain.gain().set_value(volume * volume);
            if w.playing.as_ref().map(|p| p.0) == want {
                return;
            }
            if let Some((_, node)) = w.playing.take() {
                #[allow(deprecated)]
                let _ = node.stop();
            }
            let Some((track, at)) = want else { return };
            let buf = match w.cache.iter().find(|c| c.0 == track) {
                Some(c) => c.1.clone(),
                None => {
                    let Some(b) = buffer(&w.ctx, &music::render(track)) else { return };
                    w.cache.push((track, b.clone()));
                    b
                }
            };
            let Ok(node) = w.ctx.create_buffer_source() else { return };
            node.set_buffer(Some(&buf));
            node.set_loop(true);
            if node.connect_with_audio_node(&w.gain).is_err() {
                return;
            }
            let offset = position(crate::platform::shared_now() - at, buf.length() as usize) / music::RATE as f64;
            let _ = node.start_with_when_and_grain_offset(0.0, offset);
            w.playing = Some(((track, at), node));
        });
    }

    pub fn chime(sound: crate::prefs::TimerSound, volume: f32) {
        with(|w| {
            let samples = music::chime_sound(sound);
            let Some(buf) = buffer(&w.ctx, &samples) else { return };
            let (Ok(node), Ok(g)) = (w.ctx.create_buffer_source(), w.ctx.create_gain()) else { return };
            g.gain().set_value(volume);
            node.set_buffer(Some(&buf));
            if node.connect_with_audio_node(&g).is_ok() && g.connect_with_audio_node(&w.ctx.destination()).is_ok() {
                #[allow(deprecated)]
                let _ = node.start();
            }
        });
    }
}

#[cfg(test)]
mod tests {
    /// The sound card opens and takes the music, silently:
    /// `cargo test sound_card_opens -- --ignored --nocapture` (TRATTO_VOLUME=0.3 to hear it).
    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    #[ignore]
    fn sound_card_opens() {
        let volume: f32 = std::env::var("TRATTO_VOLUME").ok().and_then(|v| v.parse().ok()).unwrap_or(0.0);
        let at = crate::platform::shared_now();
        for _ in 0..30 {
            super::music(Some((crate::music::Track::Lofi, at)), volume);
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        assert!(super::imp::running(), "the sound card is playing");
        super::chime(crate::prefs::TimerSound::Tibetan, volume);
        std::thread::sleep(std::time::Duration::from_millis(1500));
        super::music(None, volume);
    }

    #[test]
    fn everyone_is_at_the_same_point_of_the_loop() {
        let len = 22_050 * 10;
        assert_eq!(super::position(0.0, len), 0.0);
        assert_eq!(super::position(2500.0, len), 2.5 * 22_050.0);
        // Thirteen seconds into a ten-second loop: three seconds in.
        assert_eq!(super::position(13_000.0, len), 3.0 * 22_050.0);
        // A start a little in the future (another clock): from the beginning.
        assert_eq!(super::position(-40.0, len), 0.0);
    }
}
