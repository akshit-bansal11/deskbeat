//! The Inlay mark, as in `assets/inlay.svg`: four flat pieces set into one
//! tile, one for each widget, the way an inlay sets pieces into a surface.
//! Two quarter discs face each other across one diagonal and two discs sit
//! on the other.
//!
//! Everything is on the SVG's 108-unit grid. `scripts/make_icon.py` holds the
//! same numbers for the exe's icon.

use crate::color::Rgba;

/// The tile, and the radius of its corners.
pub const SIDE: f32 = 108.0;
pub const CORNER: f32 = 26.0;

/// The brand's five colours. Everything of the app's own is drawn in them.
pub const INK: Rgba = rgb(0x15, 0x15, 0x1C);
pub const BONE: Rgba = rgb(0xF3, 0xEF, 0xE6);
pub const ORANGE: Rgba = rgb(0xFF, 0x6B, 0x2C);
pub const COBALT: Rgba = rgb(0x33, 0x55, 0xFF);
pub const SUN: Rgba = rgb(0xFF, 0xC9, 0x3C);

const fn rgb(r: u8, g: u8, b: u8) -> Rgba {
    [r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, 1.0]
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Shape {
    Disc {
        centre: (f32, f32),
        radius: f32,
    },
    /// A quarter of a disc. `from` and `to` are the two ends of its arc,
    /// which runs clockwise from one to the other.
    Quarter {
        centre: (f32, f32),
        radius: f32,
        from: (f32, f32),
        to: (f32, f32),
    },
}

/// The four pieces, each in its cell of a two-by-two grid on the tile.
pub const PIECES: [(Shape, Rgba); 4] = [
    (
        Shape::Quarter {
            centre: (20.0, 52.0),
            radius: 32.0,
            from: (20.0, 20.0),
            to: (52.0, 52.0),
        },
        COBALT,
    ),
    (
        Shape::Disc {
            centre: (72.0, 36.0),
            radius: 16.0,
        },
        ORANGE,
    ),
    (
        Shape::Disc {
            centre: (36.0, 72.0),
            radius: 16.0,
        },
        SUN,
    ),
    (
        Shape::Quarter {
            centre: (88.0, 88.0),
            radius: 32.0,
            from: (56.0, 88.0),
            to: (88.0, 56.0),
        },
        BONE,
    ),
];

impl Shape {
    /// How far `(x, y)` is outside the shape, in units; negative inside.
    fn outside(self, x: f32, y: f32) -> f32 {
        match self {
            Shape::Disc { centre, radius } => (x - centre.0).hypot(y - centre.1) - radius,
            Shape::Quarter {
                centre,
                radius,
                from,
                to,
            } => {
                // The disc, cut by the two straight edges that run from the
                // centre out to the ends of the arc. A point is inside the
                // quarter when it lies along both edges' directions.
                let behind = |end: (f32, f32)| {
                    let (dx, dy) = ((end.0 - centre.0) / radius, (end.1 - centre.1) / radius);
                    -((x - centre.0) * dx + (y - centre.1) * dy)
                };
                let disc = (x - centre.0).hypot(y - centre.1) - radius;
                disc.max(behind(from)).max(behind(to))
            }
        }
    }
}

/// The colour of a pixel, straight alpha, in a mark `side` pixels across.
/// `(x, y)` is the pixel's centre.
pub fn pixel(x: f32, y: f32, side: f32) -> Rgba {
    let unit = side / SIDE;
    // Distances in units, turned into pixels and then into a coverage.
    let coverage = |outside: f32| (0.5 - outside * unit).clamp(0.0, 1.0);
    let (x, y) = (x / unit, y / unit);

    let half = SIDE / 2.0;
    let dx = (x - half).abs() - (half - CORNER);
    let dy = (y - half).abs() - (half - CORNER);
    let tile = coverage(dx.max(0.0).hypot(dy.max(0.0)) - CORNER);

    // The pieces never overlap, so each blends over the tile alone.
    let mut colour = INK;
    for (shape, piece) in PIECES {
        let cover = coverage(shape.outside(x, y));
        if cover >= 1.0 {
            colour = piece;
        } else {
            for i in 0..3 {
                colour[i] += (piece[i] - colour[i]) * cover;
            }
        }
    }
    colour[3] = tile;
    colour
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_piece_sits_in_its_own_cell_in_its_own_colour() {
        let at = |x: f32, y: f32| pixel(x, y, SIDE);
        assert_eq!(at(0.5, 0.5)[3], 0.0, "the corner is cut off");
        assert_eq!(at(54.0, 54.0), INK, "the gap between the cells");
        assert_eq!(at(30.0, 40.0), COBALT, "the quarter at the top left");
        assert_eq!(at(48.0, 22.0), INK, "outside the top left quarter's arc");
        assert_eq!(at(72.0, 36.0), ORANGE);
        assert_eq!(at(36.0, 72.0), SUN);
        assert_eq!(at(80.0, 80.0), BONE, "the quarter at the bottom right");
        assert_eq!(
            at(60.0, 60.0),
            INK,
            "outside the bottom right quarter's arc"
        );
    }

    #[test]
    fn the_pieces_keep_apart() {
        // No point within a unit of one piece is inside another.
        for row in 0..216 {
            for column in 0..216 {
                let (x, y) = (column as f32 / 2.0, row as f32 / 2.0);
                let near = PIECES
                    .iter()
                    .filter(|(shape, _)| shape.outside(x, y) < 1.0)
                    .count();
                assert!(near <= 1, "pieces meet at ({x}, {y})");
            }
        }
    }
}
