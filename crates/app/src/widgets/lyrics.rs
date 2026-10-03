//! Synced lyrics: the current line in the middle, its neighbours fading away
//! above and below, scrolling as the song moves on.

use sonic_veil_core::clock::{Anchor, Clock, find_line_index, find_word_index};
use sonic_veil_core::color::{Rgba, mix, parse_hex, with_alpha};
use sonic_veil_core::config::{Align, LyricsMode};
use sonic_veil_core::timing::Line;
use windows::Win32::Graphics::DirectWrite::IDWriteTextLayout;
use windows::core::Result;

use super::{Ctx, Tick, Wake, Widget, draw_card, named_color};
use crate::gfx::{Gfx, TextFx, TextStyle};
use crate::lyrics::Lyrics;

/// A jump of more lines than this is a seek: snap instead of scrolling past them all.
const SNAP_BEYOND_LINES: f32 = 3.5;
/// While the clock is absorbing a correction, re-check at least this often.
const SETTLING_RECHECK_MS: f64 = 50.0;

#[derive(Default)]
pub struct LyricsView {
    clock: Clock,
    last_ms: f64,
    seen_update: Option<u64>,
    seen_lyrics: Option<u64>,

    /// One layout per lyric line with its height, and the width they were built for.
    layouts: Vec<(IDWriteTextLayout, f32)>,
    /// Vertical centre of each line when all are stacked from zero.
    centres: Vec<f32>,
    layout_width: f32,

    /// The line that has most recently started.
    active: Option<usize>,
    /// Whether that line is still being sung, rather than in the gap after it.
    lit: bool,
    word: Option<usize>,
    /// Track position at the last tick, which the word fill is drawn from.
    pos: i64,

    /// The scroll position, as a fractional line index, eased from `from` to `to`.
    from: f32,
    to: f32,
    anim_start_ms: f64,
}

/// Where playback is among the lines, and when that next changes.
struct Place {
    active: Option<usize>,
    lit: bool,
    word: Option<usize>,
    /// Track position, in ms, of the next change. `i64::MAX` when there is none.
    next_change: i64,
}

fn place_synced(lines: &[Line], pos: i64, by_word: bool) -> Place {
    let active = find_line_index(lines, pos);
    let line = active.map(|i| &lines[i]);
    let lit = line.is_some_and(|l| pos < l.end_ms);
    let word = line
        .filter(|_| lit && by_word)
        .and_then(|l| find_word_index(&l.words, pos));

    let mut next_change = lines
        .get(active.map_or(0, |i| i + 1))
        .map_or(i64::MAX, |next| next.start_ms);
    if let Some(line) = line.filter(|_| lit) {
        next_change = next_change.min(line.end_ms);
        if by_word {
            // Sourced words overlap and leave gaps, so they are not in order.
            let boundary = line
                .words
                .iter()
                .flat_map(|w| [w.start_ms, w.end_ms])
                .filter(|&t| t > pos)
                .min();
            next_change = next_change.min(boundary.unwrap_or(i64::MAX));
        }
    }
    Place {
        active,
        lit,
        word,
        next_change,
    }
}

/// Unsynced lyrics have no timestamps, so the position is a guess: the lines
/// are spread evenly over the track. Nothing is highlighted, because nothing
/// is known.
fn place_plain(count: usize, pos: i64, duration_ms: f64) -> Place {
    if count == 0 || duration_ms <= 0.0 {
        return Place {
            active: None,
            lit: false,
            word: None,
            next_change: i64::MAX,
        };
    }
    let per_line = duration_ms / count as f64;
    let index = ((pos.max(0) as f64 / per_line) as usize).min(count - 1);
    Place {
        active: Some(index),
        lit: false,
        word: None,
        next_change: ((index + 1) as f64 * per_line) as i64 + 1,
    }
}

fn ease_out_cubic(t: f32) -> f32 {
    1.0 - (1.0 - t).powi(3)
}

impl LyricsView {
    fn scroll_position(&self, now_ms: f64, scroll_ms: u32) -> f32 {
        if self.from == self.to || scroll_ms == 0 {
            return self.to;
        }
        let t = ((now_ms - self.anim_start_ms) / f64::from(scroll_ms)).clamp(0.0, 1.0) as f32;
        self.from + (self.to - self.from) * ease_out_cubic(t)
    }

    fn build_layouts(&mut self, g: &mut Gfx, texts: &[&str], width: f32, ctx: &Ctx) -> Result<()> {
        let cfg = &ctx.cfg.lyrics;
        let style = TextStyle {
            font: lyric_font(g, ctx),
            size: cfg.size,
            weight: cfg.weight,
            align: cfg.align,
            wrap: true,
        };
        self.layouts.clear();
        self.centres.clear();
        let mut top = 0.0;
        for text in texts {
            let layout = g.layout(text, &style, width, 10_000.0)?;
            let height = Gfx::measure(&layout).1;
            self.centres.push(top + height / 2.0);
            self.layouts.push((layout, height));
            top += height + cfg.size * cfg.line_gap;
        }
        self.layout_width = width;
        Ok(())
    }

    fn status(&self, g: &mut Gfx, message: &str, w: f32, h: f32, ctx: &Ctx) -> Result<()> {
        let cfg = &ctx.cfg.lyrics;
        let style = TextStyle {
            font: lyric_font(g, ctx),
            size: (cfg.size * 0.6).max(13.0),
            weight: 500,
            align: cfg.align,
            wrap: true,
        };
        let pad = if cfg.card { 22.0 } else { 6.0 };
        let layout = g.layout(message, &style, w - 2.0 * pad, h)?;
        let height = Gfx::measure(&layout).1;
        let color = with_alpha(
            named_color(&cfg.inactive_color, ctx),
            cfg.inactive_opacity * cfg.opacity,
        );
        g.draw_text_fx(&layout, pad, (h - height) / 2.0, color, &decoration(ctx));
        Ok(())
    }
}

/// The lyrics font, falling back to the theme font when it is not set or
/// not available.
fn lyric_font<'a>(g: &Gfx, ctx: &Ctx<'a>) -> &'a str {
    let cfg = ctx.cfg;
    if !cfg.lyrics.font.is_empty() && g.has_font(&cfg.lyrics.font) {
        &cfg.lyrics.font
    } else {
        &cfg.theme.font
    }
}

/// The shadow and outline for lyric text, at full strength. Drawing fades
/// them together with the line they belong to.
fn decoration(ctx: &Ctx) -> TextFx {
    let cfg = &ctx.cfg.lyrics;
    let color =
        |hex: &str, alpha: f32| with_alpha(parse_hex(hex).unwrap_or([0.0, 0.0, 0.0, 1.0]), alpha);
    TextFx {
        shadow: (cfg.shadow_opacity > 0.0).then(|| color(&cfg.shadow_color, cfg.shadow_opacity)),
        stroke: (cfg.stroke_width > 0.0).then(|| (color(&cfg.stroke_color, 1.0), cfg.stroke_width)),
    }
}

/// UTF-16 offset and length of each word of a line within its text.
fn word_ranges(line: &Line) -> impl Iterator<Item = (u32, u32)> + '_ {
    line.words.iter().scan(0, |start, word| {
        let len = word.text.encode_utf16().count() as u32;
        let range = (*start, len);
        *start += len + u32::from(!word.joined);
        Some(range)
    })
}

/// Paints the sung part of the current line over its unsung text: the words
/// that are over in `sung`, and as much of each word in progress as has been
/// sung in `singing`, behind an edge `feather` wide.
fn draw_sung(
    g: &Gfx,
    layout: &IDWriteTextLayout,
    line: &Line,
    (x, y): (f32, f32),
    pos: i64,
    (sung, singing): (Rgba, Rgba),
    feather: f32,
) -> Result<()> {
    for (word, (start, len)) in line.words.iter().zip(word_ranges(line)) {
        if pos < word.start_ms {
            continue;
        }
        let boxes = Gfx::range_rects(layout, start, len);
        if pos >= word.end_ms {
            for clip in boxes {
                g.draw_text_clipped(layout, x, y, sung, clip);
            }
            continue;
        }
        let done = (pos - word.start_ms) as f32 / (word.end_ms - word.start_ms) as f32;
        // The edge travels the word's width plus its own, so the word is
        // empty when it starts and full when it ends.
        // ponytail: fills left to right, wrong for right-to-left scripts;
        // flip the gradient by the layout's reading direction if that matters.
        for clip in boxes {
            let edge = x + clip.left + done * (clip.right - clip.left + feather);
            let brush = g.gradient(edge - feather, edge, singing, with_alpha(singing, 0.0))?;
            g.draw_text_with(layout, x, y, &brush, clip);
        }
    }
    Ok(())
}

impl Widget for LyricsView {
    fn reset(&mut self) {
        self.layouts.clear();
    }

    fn tick(&mut self, ctx: &Ctx) -> Tick {
        let cfg = &ctx.cfg.lyrics;
        let media = ctx.media;
        let mut dirty = false;

        self.clock.offset_ms = f64::from(cfg.offset_ms);
        self.clock.advance((ctx.now_ms - self.last_ms).max(0.0));
        self.last_ms = ctx.now_ms;
        if self.seen_update != Some(media.update_gen) {
            self.seen_update = Some(media.update_gen);
            self.clock.apply_anchor(
                Anchor {
                    is_playing: media.playing,
                    progress_ms: media.position_ms,
                    sampled_at_ms: media.position_at_ms,
                },
                ctx.now_ms,
            );
        }

        let new_lyrics = self.seen_lyrics != Some(ctx.lyrics_gen);
        if new_lyrics {
            self.seen_lyrics = Some(ctx.lyrics_gen);
            self.layouts.clear();
            dirty = true;
        }

        let pos = self.clock.read() as i64;
        let place = match ctx.lyrics {
            Lyrics::Synced(lines) => place_synced(lines, pos, cfg.mode == LyricsMode::Word),
            Lyrics::Plain(lines) => place_plain(lines.len(), pos, media.duration_ms),
            _ => place_plain(0, pos, 0.0),
        };

        if (place.active, place.lit, place.word) != (self.active, self.lit, self.word) {
            dirty = true;
        }
        // A word in progress is filling, so every new position is a new picture.
        let filling = place.word.is_some() && pos != self.pos;
        dirty |= filling;
        self.active = place.active;
        self.lit = place.lit;
        self.word = place.word;
        self.pos = pos;

        let target = place.active.unwrap_or(0) as f32;
        if new_lyrics {
            self.from = target;
            self.to = target;
        } else if target != self.to {
            let current = self.scroll_position(ctx.now_ms, cfg.scroll_ms);
            let snap = cfg.scroll_ms == 0 || (target - current).abs() > SNAP_BEYOND_LINES;
            self.from = if snap { target } else { current };
            self.to = target;
            self.anim_start_ms = ctx.now_ms;
            dirty = true;
        }

        let scrolling = self.from != self.to;
        if scrolling {
            dirty = true;
            if ctx.now_ms - self.anim_start_ms >= f64::from(cfg.scroll_ms) {
                // The frame being drawn now is the last one of the scroll.
                self.from = self.to;
            }
        }

        let wake = if self.from != self.to || (filling && media.playing) {
            Wake::Frame
        } else if media.playing && place.next_change != i64::MAX {
            let wait = (place.next_change - pos).max(1) as f64;
            Wake::after_ms(if self.clock.is_settling() {
                wait.min(SETTLING_RECHECK_MS)
            } else {
                wait
            })
        } else {
            Wake::Idle
        };
        Tick { dirty, wake }
    }

    fn draw(&mut self, g: &mut Gfx, w: f32, h: f32, ctx: &Ctx) -> Result<()> {
        let cfg = &ctx.cfg.lyrics;
        let o = cfg.opacity;
        if cfg.card {
            draw_card(g, w, h, ctx, o);
        }

        let synced = match ctx.lyrics {
            Lyrics::Synced(lines) => Some(lines),
            _ => None,
        };
        let texts: Vec<&str> = match ctx.lyrics {
            Lyrics::Synced(lines) => lines.iter().map(|l| l.text.as_str()).collect(),
            Lyrics::Plain(lines) => lines.iter().map(String::as_str).collect(),
            Lyrics::Missing => return self.status(g, "No lyrics for this track", w, h, ctx),
            Lyrics::Instrumental => return self.status(g, "Instrumental", w, h, ctx),
            Lyrics::None | Lyrics::Loading => return Ok(()),
        };
        if texts.is_empty() {
            return Ok(());
        }

        let pad = if cfg.card { 22.0 } else { 6.0 };
        // Lines are laid out narrower than the widget by the factor the
        // centre line is enlarged by, so that line still fits once it is.
        let width = ((w - 2.0 * pad) / cfg.active_scale).max(1.0);
        if self.layouts.len() != texts.len() || self.layout_width != width {
            self.build_layouts(g, &texts, width, ctx)?;
        }
        // Where the lines sit, and the point they are enlarged about.
        let (x, anchor) = match cfg.align {
            Align::Left => (pad, pad),
            Align::Center => ((w - width) / 2.0, w / 2.0),
            Align::Right => (w - pad - width, w - pad),
        };

        let last = texts.len() - 1;
        let p = self
            .scroll_position(ctx.now_ms, cfg.scroll_ms)
            .clamp(0.0, last as f32);
        let below = (p as usize).min(last);
        let above = (below + 1).min(last);
        let scroll = self.centres[below] + (self.centres[above] - self.centres[below]) * p.fract();

        let (before, after) = (cfg.lines_before as f32, cfg.lines_after as f32);
        let inactive = with_alpha(
            named_color(&cfg.inactive_color, ctx),
            cfg.inactive_opacity * o,
        );
        let active = with_alpha(named_color(&cfg.active_color, ctx), o);
        let word_color = with_alpha(named_color(&cfg.word_color, ctx), o);
        let unsung = with_alpha(
            named_color(&cfg.inactive_color, ctx),
            cfg.unsung_opacity * o,
        );
        let first = (p - before - 1.0).max(0.0) as usize;
        let end = ((p + after + 2.0) as usize).min(last);

        for i in first..=end {
            let distance = i as f32 - p;
            // Lines fade in and out as they cross the edge of the visible range,
            // so nothing pops when the scroll brings a new line in.
            let edge = if distance < 0.0 {
                before + 1.0 + distance
            } else {
                after + 1.0 - distance
            }
            .clamp(0.0, 1.0);
            // Each line further from the centre is fainter than the last.
            let fade = edge * (1.0 - cfg.falloff).powf((distance.abs() - 1.0).max(0.0));
            let (layout, height) = &self.layouts[i];
            let y = h / 2.0 + self.centres[i] - scroll - height / 2.0;
            if fade <= 0.0 || y + height < 0.0 || y > h {
                continue;
            }

            let current = self.lit && self.active == Some(i);
            // Size and highlight arrive with the line as it scrolls to the centre.
            let nearness = (1.0 - distance.abs()).clamp(0.0, 1.0);
            let glow = if current { nearness } else { 0.0 };
            // In word mode the current line starts out unsung and is filled
            // in word by word; otherwise all of it is lit at once.
            let by_word = synced
                .filter(|_| current && cfg.mode == LyricsMode::Word)
                .map(|lines| &lines[i]);
            let lit = if by_word.is_some() { unsung } else { active };

            let scale = 1.0 + (cfg.active_scale - 1.0) * nearness;
            let saved = g.scale_about(scale, anchor, y + height / 2.0);
            let color = with_alpha(mix(inactive, lit, glow), fade);
            g.draw_text_fx(layout, x, y, color, &decoration(ctx));
            let filled = by_word.map_or(Ok(()), |line| {
                let colors = (
                    with_alpha(active, glow * fade),
                    with_alpha(word_color, glow * fade),
                );
                draw_sung(g, layout, line, (x, y), self.pos, colors, cfg.size * 0.4)
            });
            g.restore(saved);
            filled?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sonic_veil_core::lrc::parse_lrc;
    use sonic_veil_core::timing::normalize_lines;

    fn lines() -> Vec<Line> {
        let lrc = "[00:10.00] one two\n[00:12.00] three\n[00:40.00] four";
        normalize_lines(&parse_lrc(lrc).lines, 60_000)
    }

    #[test]
    fn before_the_first_line_nothing_is_lit_and_the_wait_is_for_it() {
        let place = place_synced(&lines(), 4000, false);
        assert_eq!((place.active, place.lit), (None, false));
        assert_eq!(place.next_change, 10_000);
    }

    #[test]
    fn a_line_is_lit_until_it_ends_then_waits_for_the_next() {
        let lines = lines();
        let singing = place_synced(&lines, 12_500, false);
        assert_eq!((singing.active, singing.lit), (Some(1), true));
        // Line 1 is capped at 8s, long before line 2 starts at 40s.
        assert_eq!(singing.next_change, 20_000);

        let gap = place_synced(&lines, 25_000, false);
        assert_eq!((gap.active, gap.lit), (Some(1), false));
        assert_eq!(gap.next_change, 40_000);
    }

    #[test]
    fn word_mode_wakes_at_word_boundaries() {
        let lines = lines();
        let place = place_synced(&lines, 10_000, true);
        assert_eq!(place.word, Some(0));
        assert_eq!(place.next_change, lines[0].words[0].end_ms);
        assert_eq!(place_synced(&lines, 10_000, false).word, None);
    }

    #[test]
    fn after_the_last_line_there_is_nothing_left_to_wait_for() {
        let place = place_synced(&lines(), 59_000, false);
        assert_eq!((place.active, place.lit), (Some(2), false));
        assert_eq!(place.next_change, i64::MAX);
    }

    #[test]
    fn plain_lyrics_are_spread_evenly_over_the_track() {
        let place = place_plain(10, 25_000, 100_000.0);
        assert_eq!((place.active, place.lit), (Some(2), false));
        assert_eq!(place.next_change, 30_001);
        assert_eq!(place_plain(10, 999_999, 100_000.0).active, Some(9));
        assert_eq!(place_plain(0, 0, 100_000.0).active, None);
        assert_eq!(place_plain(5, 0, 0.0).active, None);
    }

    #[test]
    fn word_ranges_count_utf16_units() {
        let mut lines = normalize_lines(&parse_lrc("[00:01.00] héllo 😀 end").lines, 9000);
        let ranges = |line: &Line| word_ranges(line).collect::<Vec<_>>();
        assert_eq!(ranges(&lines[0]), [(0, 5), (6, 2), (9, 3)]);
        // A syllable joined to the next has no space after it.
        lines[0].words[0].joined = true;
        assert_eq!(ranges(&lines[0]), [(0, 5), (5, 2), (8, 3)]);
    }
}
