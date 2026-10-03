//! The spectrum visualizer. Runs only while there is sound, or while the
//! bars are still falling after it stops.

use std::sync::Arc;
use std::sync::atomic::Ordering;

use rustfft::num_complex::Complex;
use rustfft::{Fft, FftPlanner};
use sonic_veil_core::bands::{BandMap, hann_window, settle};
use sonic_veil_core::color::{Rgba, mix, parse_hex};
use sonic_veil_core::config::{VisualizerCfg, VisualizerColor, VisualizerStyle};
use windows::Win32::Graphics::Direct2D::ID2D1LinearGradientBrush;
use windows::core::Result;

use super::{Ctx, Tick, Wake, Widget, draw_card};
use crate::gfx::{Gfx, rect};

const FFT_SIZE: usize = 2048;
/// Bars below this are at rest; once all are, the widget stops animating.
const AT_REST: f32 = 0.003;
/// A tick arriving this much before a frame is due still counts as due,
/// so display refresh jitter does not halve the frame rate.
const FRAME_SLACK_MS: f64 = 1.5;
/// Longest step the bar physics will take, so a stall does not teleport bars.
const MAX_STEP_MS: f64 = 100.0;
const MIN_BAR_HEIGHT: f32 = 3.0;

pub struct Visualizer {
    fft: Arc<dyn Fft<f32>>,
    window: Vec<f32>,
    /// Scales a windowed FFT magnitude so a full-scale sine reads 1.0.
    norm: f32,
    samples: Vec<f32>,
    spectrum: Vec<Complex<f32>>,
    scratch: Vec<Complex<f32>>,
    magnitudes: Vec<f32>,

    /// Rebuilt when the band count, sample rate or frequency range changes.
    map: Option<(BandMap, [f32; 5])>,
    targets: Vec<f32>,
    levels: Vec<f32>,

    last_ms: f64,
    moving: bool,
    brush: Option<(ID2D1LinearGradientBrush, [f32; 10])>,
}

impl Visualizer {
    pub fn new() -> Self {
        let fft = FftPlanner::new().plan_fft_forward(FFT_SIZE);
        let (window, norm) = hann_window(FFT_SIZE);
        Self {
            scratch: vec![Complex::default(); fft.get_inplace_scratch_len()],
            fft,
            window,
            norm,
            samples: vec![0.0; FFT_SIZE],
            spectrum: vec![Complex::default(); FFT_SIZE],
            magnitudes: vec![0.0; FFT_SIZE / 2],
            map: None,
            targets: Vec::new(),
            levels: Vec::new(),
            last_ms: 0.0,
            moving: false,
            brush: None,
        }
    }

    /// How many distinct bands feed the bars. A symmetric layout shows each twice.
    fn band_count(cfg: &VisualizerCfg) -> usize {
        let bars = cfg.bars as usize;
        if cfg.symmetric {
            bars.div_ceil(2)
        } else {
            bars
        }
    }

    fn analyse(&mut self, ctx: &Ctx) {
        let cfg = &ctx.cfg.visualizer;
        let sample_rate = ctx.audio.latest(&mut self.samples) as f32;
        let bands = self.levels.len();

        let key = [bands as f32, sample_rate, cfg.min_hz, cfg.max_hz, cfg.tilt];
        if self
            .map
            .as_ref()
            .is_none_or(|(_, built_for)| *built_for != key)
        {
            let map = BandMap::new(
                bands,
                FFT_SIZE,
                sample_rate,
                cfg.min_hz,
                cfg.max_hz,
                cfg.tilt,
            );
            self.map = Some((map, key));
        }

        for ((bin, &sample), &weight) in self
            .spectrum
            .iter_mut()
            .zip(&self.samples)
            .zip(&self.window)
        {
            *bin = Complex::new(sample * weight, 0.0);
        }
        self.fft
            .process_with_scratch(&mut self.spectrum, &mut self.scratch);
        for (magnitude, bin) in self.magnitudes.iter_mut().zip(&self.spectrum) {
            *magnitude = bin.norm() * self.norm;
        }
        if let Some((map, _)) = &self.map {
            map.levels(&self.magnitudes, cfg.sensitivity, &mut self.targets);
        }
    }

    /// The level shown by bar `i` of `bars`.
    fn bar_level(&self, i: usize, bars: usize, symmetric: bool) -> f32 {
        let band = if symmetric {
            // Bass in the middle, treble at both edges.
            let half = bars / 2;
            if i < half { half - 1 - i } else { i - half }
        } else {
            i
        };
        self.levels.get(band).copied().unwrap_or(0.0)
    }
}

fn colors(cfg: &VisualizerCfg, ctx: &Ctx) -> (Rgba, Rgba) {
    let hex = |s: &str| parse_hex(s).unwrap_or(ctx.accent);
    match cfg.color {
        VisualizerColor::Accent => (ctx.accent, mix(ctx.accent, [1.0; 4], 0.45)),
        VisualizerColor::Solid => (hex(&cfg.color_a), hex(&cfg.color_a)),
        VisualizerColor::Gradient => (hex(&cfg.color_a), hex(&cfg.color_b)),
    }
}

impl Widget for Visualizer {
    fn reset(&mut self) {
        self.map = None;
        self.brush = None;
    }

    fn tick(&mut self, ctx: &Ctx) -> Tick {
        let cfg = &ctx.cfg.visualizer;
        let bands = Self::band_count(cfg);
        if self.levels.len() != bands {
            self.levels = vec![0.0; bands];
            self.targets = vec![0.0; bands];
        }

        let hearing = ctx.audio.active.load(Ordering::SeqCst);
        if !hearing && self.levels.iter().all(|&level| level < AT_REST) {
            // At rest. One last draw settles the bars flat, then nothing runs
            // until the capture thread reports sound again.
            let dirty = self.moving;
            self.moving = false;
            self.levels.fill(0.0);
            self.last_ms = ctx.now_ms;
            return Tick {
                dirty,
                wake: Wake::Idle,
            };
        }

        let elapsed = ctx.now_ms - self.last_ms;
        if self.moving && elapsed < 1000.0 / f64::from(cfg.fps) - FRAME_SLACK_MS {
            // The display refreshes faster than the configured frame rate.
            return Tick {
                dirty: false,
                wake: Wake::Frame,
            };
        }
        self.last_ms = ctx.now_ms;
        self.moving = true;

        if hearing {
            self.analyse(ctx);
        } else {
            self.targets.fill(0.0);
        }
        let step = elapsed.clamp(0.0, MAX_STEP_MS) as f32;
        for (level, &target) in self.levels.iter_mut().zip(&self.targets) {
            *level = settle(*level, target, step, cfg.attack_ms, cfg.decay_ms);
        }
        Tick {
            dirty: true,
            wake: Wake::Frame,
        }
    }

    fn draw(&mut self, g: &mut Gfx, w: f32, h: f32, ctx: &Ctx) -> Result<()> {
        let cfg = &ctx.cfg.visualizer;
        if cfg.card {
            draw_card(g, w, h, ctx, cfg.opacity);
        }
        let pad = if cfg.card { 18.0 } else { 0.0 };
        let (left, top, width, height) = (pad, pad, w - 2.0 * pad, h - 2.0 * pad);
        let bars = cfg.bars as usize;
        if width <= 0.0 || height <= 0.0 || bars == 0 {
            return Ok(());
        }

        let (a, b) = colors(cfg, ctx);
        let key = [a[0], a[1], a[2], a[3], b[0], b[1], b[2], b[3], left, width];
        if self
            .brush
            .as_ref()
            .is_none_or(|(_, built_for)| *built_for != key)
        {
            self.brush = Some((g.gradient(left, left + width, a, b)?, key));
        }
        let Some((brush, _)) = &self.brush else {
            return Ok(());
        };
        unsafe { brush.SetOpacity(cfg.opacity) };

        let pitch = width / bars as f32;
        let bar_width = (pitch * (1.0 - cfg.gap)).max(1.0);
        let radius = bar_width * cfg.radius;
        let floor = MIN_BAR_HEIGHT.min(height);

        // Flipping sideways reads the bands in reverse; flipping upside down
        // hangs the shape from the top edge instead of standing it on the bottom.
        let level_at = |i: usize| {
            let slot = if cfg.flip_x { bars - 1 - i } else { i };
            self.bar_level(slot, bars, cfg.symmetric)
        };
        let base = if cfg.flip_y { top } else { top + height };
        let rise = if cfg.flip_y { 1.0 } else { -1.0 };

        if cfg.style == VisualizerStyle::Wave {
            let mut points = Vec::with_capacity(bars + 2);
            points.push((left, base));
            for i in 0..bars {
                points.push((
                    left + pitch * (i as f32 + 0.5),
                    base + rise * (level_at(i) * height).max(floor),
                ));
            }
            points.push((left + width, base));
            return g.fill_curve(&points, base, brush);
        }

        for i in 0..bars {
            let bar_height = (level_at(i) * height).max(floor);
            let x = left + pitch * i as f32 + (pitch - bar_width) / 2.0;
            let y = match cfg.style {
                VisualizerStyle::Mirror => top + (height - bar_height) / 2.0,
                _ if cfg.flip_y => top,
                _ => top + height - bar_height,
            };
            g.fill_round_with(
                rect(x, y, bar_width, bar_height),
                radius.min(bar_height / 2.0),
                brush,
            );
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_symmetric_layout_puts_bass_in_the_middle() {
        let mut vis = Visualizer::new();
        vis.levels = vec![1.0, 0.5, 0.25, 0.1];
        let shown: Vec<f32> = (0..8).map(|i| vis.bar_level(i, 8, true)).collect();
        assert_eq!(shown, [0.1, 0.25, 0.5, 1.0, 1.0, 0.5, 0.25, 0.1]);
        // An odd bar count has one more band than half, for the last bar.
        vis.levels = vec![1.0, 0.5, 0.25];
        let odd: Vec<f32> = (0..5).map(|i| vis.bar_level(i, 5, true)).collect();
        assert_eq!(odd, [0.5, 1.0, 1.0, 0.5, 0.25]);
    }

    #[test]
    fn a_plain_layout_shows_each_band_once() {
        let mut vis = Visualizer::new();
        vis.levels = vec![0.3, 0.6];
        assert_eq!(vis.bar_level(1, 2, false), 0.6);
        assert_eq!(vis.bar_level(9, 2, false), 0.0);
    }
}
