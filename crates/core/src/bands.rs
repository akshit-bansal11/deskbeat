//! Maps FFT magnitudes onto log-spaced visualizer bands.

/// Level 0.0 sits at this many dBFS, before gain.
const FLOOR_DB: f32 = -66.0;
/// Level 1.0 sits at this many dBFS, before gain.
const CEIL_DB: f32 = -14.0;
/// The tilt pivots here: bands above get a boost, bands below a cut.
const TILT_PIVOT_HZ: f32 = 1000.0;

/// A Hann window and the factor that scales a windowed FFT magnitude so a
/// full-scale sine reads 1.0.
pub fn hann_window(size: usize) -> (Vec<f32>, f32) {
    let window: Vec<f32> = (0..size)
        .map(|i| {
            let phase = std::f32::consts::TAU * i as f32 / size as f32;
            0.5 - 0.5 * phase.cos()
        })
        .collect();
    let norm = 2.0 / window.iter().sum::<f32>();
    (window, norm)
}

/// Which FFT bins feed each band, precomputed for one FFT size and sample rate.
pub struct BandMap {
    /// Fractional bin range `[lo, hi)` covered by each band.
    ranges: Vec<(f32, f32)>,
    /// dB added to each band: music loses energy toward the treble, and
    /// without a tilt the right half of a spectrum barely moves.
    tilt_db: Vec<f32>,
}

impl BandMap {
    pub fn new(
        bands: usize,
        fft_size: usize,
        sample_rate: f32,
        min_hz: f32,
        max_hz: f32,
        tilt_db_per_octave: f32,
    ) -> Self {
        let bin_hz = sample_rate / fft_size as f32;
        let max_hz = max_hz.min(sample_rate / 2.0);
        let min_hz = min_hz.clamp(bin_hz, max_hz);
        let ratio = max_hz / min_hz;
        let edge = |i: usize| min_hz * ratio.powf(i as f32 / bands as f32);

        let ranges = (0..bands)
            .map(|i| (edge(i) / bin_hz, edge(i + 1) / bin_hz))
            .collect();
        let tilt_db = (0..bands)
            .map(|i| {
                let centre = (edge(i) * edge(i + 1)).sqrt();
                tilt_db_per_octave * (centre / TILT_PIVOT_HZ).log2()
            })
            .collect();
        Self { ranges, tilt_db }
    }

    pub fn len(&self) -> usize {
        self.ranges.len()
    }

    pub fn is_empty(&self) -> bool {
        self.ranges.is_empty()
    }

    /// Writes one level in `0.0..=1.0` per band into `out`.
    ///
    /// `magnitudes` are linear, one per FFT bin, scaled so full scale is 1.0.
    pub fn levels(&self, magnitudes: &[f32], gain_db: f32, out: &mut [f32]) {
        let Some(last) = magnitudes.len().checked_sub(1) else {
            out.fill(0.0);
            return;
        };
        let at = |bin: usize| magnitudes[bin.min(last)];

        for ((&(lo, hi), &tilt), level) in self.ranges.iter().zip(&self.tilt_db).zip(out) {
            let magnitude = if hi - lo <= 1.0 {
                // Narrower than one bin, as every bass band is: interpolate, or
                // neighbouring bands read the same bin and move as one block.
                let centre = (lo + hi) / 2.0;
                let frac = centre.fract();
                at(centre as usize) * (1.0 - frac) + at(centre as usize + 1) * frac
            } else {
                (lo as usize..hi.ceil() as usize)
                    .map(at)
                    .fold(0.0, f32::max)
            };
            let db = 20.0 * magnitude.max(1e-9).log10() + gain_db + tilt;
            *level = ((db - FLOOR_DB) / (CEIL_DB - FLOOR_DB)).clamp(0.0, 1.0);
        }
    }
}

/// Moves a bar from `current` toward `target` over `dt_ms` of time.
///
/// This one function is how the visualizer feels: a short attack makes bars
/// snap up on a hit, a long decay makes them fall like they have weight.
///
/// TODO(you): this is the default, and it is yours to rewrite. Ideas: gravity
/// (accelerate while falling), a hold before the fall, or overshoot on attack.
pub fn settle(current: f32, target: f32, dt_ms: f32, attack_ms: f32, decay_ms: f32) -> f32 {
    let tau = if target > current {
        attack_ms
    } else {
        decay_ms
    };
    if tau <= 0.0 {
        return target;
    }
    current + (target - current) * (1.0 - (-dt_ms / tau).exp())
}

#[cfg(test)]
mod tests {
    use super::*;

    const FFT: usize = 2048;
    const RATE: f32 = 48_000.0;

    fn tone(hz: f32) -> Vec<f32> {
        let mut magnitudes = vec![0.0; FFT / 2];
        magnitudes[(hz / (RATE / FFT as f32)).round() as usize] = 0.5;
        magnitudes
    }

    fn loudest(levels: &[f32]) -> usize {
        (0..levels.len())
            .max_by(|&a, &b| levels[a].total_cmp(&levels[b]))
            .unwrap()
    }

    #[test]
    fn a_tone_lights_the_band_that_contains_it() {
        let map = BandMap::new(48, FFT, RATE, 40.0, 16_000.0, 0.0);
        let mut levels = vec![0.0; map.len()];

        map.levels(&tone(100.0), 0.0, &mut levels);
        let bass = loudest(&levels);
        map.levels(&tone(5000.0), 0.0, &mut levels);
        let treble = loudest(&levels);

        assert!(bass < 12, "100 Hz landed in band {bass}");
        assert!(treble > 30, "5 kHz landed in band {treble}");
        assert!(levels.iter().all(|l| (0.0..=1.0).contains(l)));
    }

    #[test]
    fn neighbouring_bass_bands_do_not_read_identically() {
        let map = BandMap::new(64, FFT, RATE, 30.0, 16_000.0, 0.0);
        let mut magnitudes = vec![0.0; FFT / 2];
        magnitudes[2] = 0.5;
        magnitudes[3] = 0.05;
        let mut levels = vec![0.0; map.len()];
        map.levels(&magnitudes, 0.0, &mut levels);
        assert!(levels[..8].windows(2).any(|pair| pair[0] != pair[1]));
    }

    #[test]
    fn silence_is_zero_and_gain_raises_levels() {
        let map = BandMap::new(16, FFT, RATE, 40.0, 16_000.0, 0.0);
        let mut quiet = vec![1.0; 16];
        map.levels(&vec![0.0; FFT / 2], 0.0, &mut quiet);
        assert!(quiet.iter().all(|&l| l == 0.0));

        let mut soft = vec![0.0; 16];
        let mut loud = vec![0.0; 16];
        let magnitudes = vec![0.003; FFT / 2];
        map.levels(&magnitudes, 0.0, &mut soft);
        map.levels(&magnitudes, 12.0, &mut loud);
        assert!(loud[4] > soft[4]);
    }

    #[test]
    fn tilt_boosts_treble_and_cuts_bass() {
        let map = BandMap::new(16, FFT, RATE, 40.0, 16_000.0, 3.0);
        assert!(map.tilt_db[0] < 0.0);
        assert!(map.tilt_db[15] > 0.0);
    }

    #[test]
    fn an_empty_spectrum_does_not_panic() {
        let map = BandMap::new(8, FFT, RATE, 40.0, 16_000.0, 0.0);
        let mut levels = vec![0.5; 8];
        map.levels(&[], 0.0, &mut levels);
        assert!(levels.iter().all(|&l| l == 0.0));
    }

    #[test]
    fn the_window_normalises_a_full_scale_sine_to_one() {
        let (window, norm) = hann_window(FFT);
        // A sine at an exact bin frequency puts sum(window)/2 into that bin.
        let bin_magnitude = window.iter().sum::<f32>() / 2.0;
        assert!((bin_magnitude * norm - 1.0).abs() < 1e-4);
    }

    #[test]
    fn settle_rises_fast_and_falls_slowly() {
        let up = settle(0.0, 1.0, 16.0, 20.0, 300.0);
        let down = 1.0 - settle(1.0, 0.0, 16.0, 20.0, 300.0);
        assert!(up > 0.5);
        assert!(down < 0.1);
        assert_eq!(settle(0.2, 0.9, 16.0, 0.0, 0.0), 0.9);
    }
}
