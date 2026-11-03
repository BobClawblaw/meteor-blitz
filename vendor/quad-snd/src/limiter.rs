//! Meteor Blitz patch: a look-ahead peak limiter on the mixed output.
//!
//! The mixer adds every playing sound together, and the backends hard-clip
//! anything past full scale, which is heard as crackle when several loud
//! effects overlap. The limiter leaves the mix untouched while its peaks stay
//! under `CEILING`; above it the gain comes down smoothly over `LOOKAHEAD`
//! frames, reaching the needed reduction exactly as the loud frame leaves the
//! delay line (`DELAY` frames, ~2.9 ms), so no sample goes past the ceiling. It then recovers over
//! roughly `RELEASE_SECONDS`.

/// Highest output peak (about -0.4 dBFS).
const CEILING: f32 = 0.955;
/// Length of the gain's attack ramp (~2.9 ms).
const LOOKAHEAD: usize = 128;
/// Frames of delay: the ramp's last frame is the loud frame itself.
const DELAY: usize = LOOKAHEAD - 1;
/// Time for the gain to recover most of the way after a peak.
const RELEASE_SECONDS: f32 = 0.08;
const RATE: f32 = 44100.0;

pub struct Limiter {
    /// Delayed stereo frames.
    delay: Vec<[f32; 2]>,
    delay_pos: usize,
    /// Gain each of the last `LOOKAHEAD` input frames needs.
    need: Vec<f32>,
    /// The released minimum of `need`, for the last `LOOKAHEAD` frames.
    held: Vec<f32>,
    held_sum: f64,
    hold: f32,
    pos: usize,
    release: f32,
}

impl Limiter {
    pub fn new() -> Limiter {
        Limiter {
            delay: vec![[0.0; 2]; DELAY],
            delay_pos: 0,
            need: vec![1.0; LOOKAHEAD],
            held: vec![1.0; LOOKAHEAD],
            held_sum: LOOKAHEAD as f64,
            hold: 1.0,
            pos: 0,
            release: 1.0 - (-1.0 / (RELEASE_SECONDS * RATE)).exp(),
        }
    }

    /// Limits interleaved stereo samples in place.
    pub fn process(&mut self, buffer: &mut [f32]) {
        for frame in buffer.chunks_exact_mut(2) {
            let peak = frame[0].abs().max(frame[1].abs());
            self.need[self.pos] = if peak > CEILING { CEILING / peak } else { 1.0 };

            // The gain must be at or below every need still in the delay
            // line; above that it rises back towards 1 at the release rate.
            let window = self.need.iter().copied().fold(1.0f32, f32::min);
            self.hold = window.min(self.hold + (1.0 - self.hold) * self.release);

            // Averaging the held gain over the delay length turns its steps
            // into ramps that end exactly when the frame that needed them
            // comes out.
            self.held_sum += (self.hold - self.held[self.pos]) as f64;
            self.held[self.pos] = self.hold;
            let gain = (self.held_sum / LOOKAHEAD as f64) as f32;

            // The delay slot holds the oldest frame: take it out, put this
            // one in.
            let out = std::mem::replace(&mut self.delay[self.delay_pos], [frame[0], frame[1]]);
            self.delay_pos = (self.delay_pos + 1) % DELAY;
            frame[0] = out[0] * gain;
            frame[1] = out[1] * gain;
            self.pos = (self.pos + 1) % LOOKAHEAD;
        }
        // The running sum drifts by rounding; resync it once per buffer.
        self.held_sum = self.held.iter().map(|&g| g as f64).sum();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(input: &[f32]) -> Vec<f32> {
        let mut limiter = Limiter::new();
        let mut buf: Vec<f32> = input.iter().flat_map(|&s| [s, s]).collect();
        buf.extend(std::iter::repeat(0.0).take(DELAY * 2));
        // In callback-sized pieces, as the backends call it.
        for chunk in buf.chunks_mut(512) {
            limiter.process(chunk);
        }
        buf.chunks(2).skip(DELAY).map(|f| f[0]).collect()
    }

    #[test]
    fn quiet_audio_passes_unchanged() {
        let input: Vec<f32> = (0..5000).map(|i| (i as f32 * 0.05).sin() * 0.9).collect();
        let out = run(&input);
        for (a, b) in input.iter().zip(&out) {
            assert!((a - b).abs() < 1e-6);
        }
    }

    #[test]
    fn loud_audio_never_passes_the_ceiling() {
        // Three loud sines summed, with a sudden onset, like stacked blasts.
        let input: Vec<f32> = (0..20000)
            .map(|i| {
                let t = i as f32;
                if i < 3000 {
                    0.1 * (t * 0.03).sin()
                } else {
                    0.9 * ((t * 0.03).sin() + (t * 0.071).sin() + (t * 0.013).sin())
                }
            })
            .collect();
        let out = run(&input);
        let peak = out.iter().fold(0.0f32, |m, s| m.max(s.abs()));
        assert!(peak <= CEILING + 1e-4, "peak {}", peak);
        // Before the onset's ramp starts the audio is untouched.
        for (a, b) in input[..3000 - LOOKAHEAD].iter().zip(&out) {
            assert!((a - b).abs() < 1e-6);
        }
    }
}
