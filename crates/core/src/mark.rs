//! The Deskbeat mark, as in `assets/deskbeat-dark.svg`: a sun coming up off a
//! desk, its rays a spectrum caught mid-song. Your desktop is the horizon and
//! the music is what rises over it; the sun is the one thing in the accent.
//!
//! Everything is on the SVG's 108-unit grid. `scripts/make_icon.py` holds the
//! same numbers for the exe's icon.

/// The tile, and the radius of its corners.
pub const SIDE: f32 = 108.0;
pub const CORNER: f32 = 26.0;
/// The sun's centre, which is also the horizon it is cut off at, and its radius.
pub const SUN: (f32, f32) = (54.0, 72.0);
pub const SUN_RADIUS: f32 = 15.0;
/// The desk: a capsule between two centres.
pub const DESK: [(f32, f32); 2] = [(24.0, 83.0), (84.0, 83.0)];
pub const DESK_WIDTH: f32 = 12.0;
pub const RAY_WIDTH: f32 = 10.0;
/// How far from the sun's centre every ray starts.
const RAY_FROM: f32 = 23.0;
/// Each ray's angle in degrees, anticlockwise from pointing right, and its
/// length. The lengths are uneven on purpose: a spectrum, not a sunburst.
const RAYS: [(f32, f32); 5] = [
    (162.0, 11.0),
    (126.0, 20.0),
    (90.0, 27.0),
    (54.0, 22.0),
    (18.0, 14.0),
];

/// Each ray's two end centres. They are capsules `RAY_WIDTH` across.
pub fn rays() -> impl Iterator<Item = [(f32, f32); 2]> {
    RAYS.iter().map(|&(degrees, length)| {
        let (sin, cos) = degrees.to_radians().sin_cos();
        let at = |r: f32| (SUN.0 + r * cos, SUN.1 - r * sin);
        [at(RAY_FROM), at(RAY_FROM + length)]
    })
}

/// How far `(x, y)` is outside a capsule; negative inside.
fn capsule(x: f32, y: f32, [a, b]: [(f32, f32); 2], width: f32) -> f32 {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let t = (((x - a.0) * dx + (y - a.1) * dy) / (dx * dx + dy * dy)).clamp(0.0, 1.0);
    (x - a.0 - t * dx).hypot(y - a.1 - t * dy) - width / 2.0
}

/// How much of a pixel the shapes cover, in a mark `side` pixels across:
/// the tile, the ink (desk and rays) on it, and the sun on it. `(x, y)` is
/// the pixel's centre. The ink and the sun never overlap.
pub fn cover(x: f32, y: f32, side: f32) -> (f32, f32, f32) {
    let unit = side / SIDE;
    // Distances in units, turned into pixels and then into a coverage.
    let coverage = |outside: f32| (0.5 - outside * unit).clamp(0.0, 1.0);
    let (x, y) = (x / unit, y / unit);

    let half = SIDE / 2.0;
    let dx = (x - half).abs() - (half - CORNER);
    let dy = (y - half).abs() - (half - CORNER);
    let tile = dx.max(0.0).hypot(dy.max(0.0)) - CORNER;

    let ink = rays()
        .map(|ray| capsule(x, y, ray, RAY_WIDTH))
        .fold(capsule(x, y, DESK, DESK_WIDTH), f32::min);
    // A disc with everything below the horizon cut away.
    let sun = ((x - SUN.0).hypot(y - SUN.1) - SUN_RADIUS).max(y - SUN.1);

    (coverage(tile), coverage(ink), coverage(sun))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_sun_sits_on_the_horizon_under_the_rays_and_over_the_desk() {
        let side = 108.0;
        let at = |x: f32, y: f32| cover(x, y, side);
        assert_eq!(at(0.5, 0.5).0, 0.0, "the corner is cut off");
        assert_eq!(at(54.0, 10.0), (1.0, 0.0, 0.0), "bare tile above the rays");
        assert_eq!(at(54.0, 64.0), (1.0, 0.0, 1.0), "the sun");
        assert_eq!(at(54.0, 76.0), (1.0, 0.0, 0.0), "nothing under the horizon");
        assert_eq!(at(54.0, 83.0), (1.0, 1.0, 0.0), "the desk");
        assert_eq!(at(54.0, 30.0), (1.0, 1.0, 0.0), "the middle ray");
    }

    #[test]
    fn the_rays_are_uneven_and_never_touch_the_sun_or_each_other() {
        let lengths: Vec<f32> = rays().map(|[a, b]| (b.0 - a.0).hypot(b.1 - a.1)).collect();
        assert!(lengths.windows(2).any(|w| (w[0] - w[1]).abs() > 1.0));
        // The gap between neighbours at their inner ends, which are closest.
        let starts: Vec<_> = rays().map(|[a, _]| a).collect();
        for pair in starts.windows(2) {
            let gap = (pair[0].0 - pair[1].0).hypot(pair[0].1 - pair[1].1) - RAY_WIDTH;
            assert!(gap > 2.0, "rays {pair:?} are {gap} apart");
        }
        const { assert!(RAY_FROM - RAY_WIDTH / 2.0 > SUN_RADIUS + 2.0) };
        const { assert!(DESK[0].1 - DESK_WIDTH / 2.0 > SUN.1 + 2.0) };
    }
}
