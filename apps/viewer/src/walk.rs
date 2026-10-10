//! A placeholder Hero walking on a region: movement limited to the navigation's
//! walkable cells, height from the Lev heightmaps.

use glam::{Vec2, Vec3};

/// Side of a walkability cell in world units (the smallest navigation leaf).
pub const CELL: f32 = 0.5;
/// Walking speed in world units (metres) per second.
pub const WALK_SPEED: f32 = 4.0;
pub const RUN_SPEED: f32 = 8.0;

/// Which 0.5 m cells of a region are walkable.
#[derive(Debug, Clone)]
pub struct WalkGrid {
    origin: Vec2,
    size: [u32; 2],
    cells: Vec<bool>,
}

impl WalkGrid {
    /// An all-blocked grid covering `lo`..`hi`.
    pub fn new(lo: Vec2, hi: Vec2) -> Self {
        let size = [
            ((hi.x - lo.x) / CELL).ceil().max(1.0) as u32,
            ((hi.y - lo.y) / CELL).ceil().max(1.0) as u32,
        ];
        Self {
            origin: lo,
            size,
            cells: vec![false; size[0] as usize * size[1] as usize],
        }
    }

    /// Marks the square of side `side` centred on `centre` walkable.
    pub fn mark(&mut self, centre: Vec2, side: f32) {
        let lo = ((centre - Vec2::splat(side / 2.0) - self.origin) / CELL).round();
        let hi = ((centre + Vec2::splat(side / 2.0) - self.origin) / CELL).round();
        for y in lo.y.max(0.0) as u32..(hi.y.max(0.0) as u32).min(self.size[1]) {
            for x in lo.x.max(0.0) as u32..(hi.x.max(0.0) as u32).min(self.size[0]) {
                self.cells[(y * self.size[0] + x) as usize] = true;
            }
        }
    }

    pub fn walkable(&self, p: Vec2) -> bool {
        let c = ((p - self.origin) / CELL).floor();
        c.x >= 0.0
            && c.y >= 0.0
            && (c.x as u32) < self.size[0]
            && (c.y as u32) < self.size[1]
            && self.cells[(c.y as u32 * self.size[0] + c.x as u32) as usize]
    }

    /// The walkable cell centre nearest to `p`, searching outwards ring by ring.
    pub fn nearest_walkable(&self, p: Vec2) -> Option<Vec2> {
        let c = ((p - self.origin) / CELL).floor();
        let (cx, cy) = (c.x as i64, c.y as i64);
        let max = self.size[0].max(self.size[1]) as i64;
        for r in 0..max {
            let mut best: Option<(f32, Vec2)> = None;
            for y in cy - r..=cy + r {
                for x in cx - r..=cx + r {
                    if (x - cx).abs() != r && (y - cy).abs() != r {
                        continue;
                    }
                    let q = self.origin + (Vec2::new(x as f32, y as f32) + 0.5) * CELL;
                    if self.walkable(q) {
                        let d = q.distance_squared(p);
                        if best.is_none_or(|(bd, _)| d < bd) {
                            best = Some((d, q));
                        }
                    }
                }
            }
            if let Some((_, q)) = best {
                return Some(q);
            }
        }
        None
    }
}

/// Ground heights on a 1-unit grid, sampled bilinearly.
#[derive(Debug, Clone)]
pub struct HeightGrid {
    origin: Vec2,
    size: [u32; 2],
    heights: Vec<f32>,
}

impl HeightGrid {
    pub fn new(origin: Vec2, size: [u32; 2]) -> Self {
        Self {
            origin,
            size,
            heights: vec![0.0; size[0] as usize * size[1] as usize],
        }
    }

    /// Sets the height at grid point (x, y) counted from the origin.
    pub fn set(&mut self, x: u32, y: u32, h: f32) {
        if x < self.size[0] && y < self.size[1] {
            self.heights[(y * self.size[0] + x) as usize] = h;
        }
    }

    pub fn height(&self, p: Vec2) -> f32 {
        let g = p - self.origin;
        let max = Vec2::new(self.size[0] as f32 - 1.0, self.size[1] as f32 - 1.0);
        let g = g.clamp(Vec2::ZERO, max);
        let (x0, y0) = (g.x.floor() as u32, g.y.floor() as u32);
        let (x1, y1) = (
            (x0 + 1).min(self.size[0] - 1),
            (y0 + 1).min(self.size[1] - 1),
        );
        let f = g - Vec2::new(x0 as f32, y0 as f32);
        let at = |x: u32, y: u32| self.heights[(y * self.size[0] + x) as usize];
        let a = at(x0, y0) * (1.0 - f.x) + at(x1, y0) * f.x;
        let b = at(x0, y1) * (1.0 - f.x) + at(x1, y1) * f.x;
        a * (1.0 - f.y) + b * f.y
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Hero {
    pub position: Vec3,
    /// Facing angle around z, radians; 0 faces +y.
    pub heading: f32,
}

impl Hero {
    pub fn new(at: Vec2, heights: &HeightGrid) -> Self {
        Self {
            position: at.extend(heights.height(at)),
            heading: 0.0,
        }
    }

    /// Moves along `direction` (world x/y, any length) at `speed` for `dt` seconds,
    /// staying on walkable cells: if the full step is blocked, tries each axis alone
    /// so the Hero slides along edges.
    pub fn step(
        &mut self,
        direction: Vec2,
        speed: f32,
        dt: f32,
        walk: &WalkGrid,
        heights: &HeightGrid,
    ) {
        let Some(dir) = direction.try_normalize() else {
            return;
        };
        let from = self.position.truncate();
        let delta = dir * speed * dt;
        let candidates = [delta, Vec2::new(delta.x, 0.0), Vec2::new(0.0, delta.y)];
        if let Some(to) = candidates
            .iter()
            .map(|d| from + *d)
            .find(|to| *to != from && walk.walkable(*to))
        {
            self.position = to.extend(heights.height(to));
        }
        self.heading = (-dir.x).atan2(dir.y);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn corridor() -> (WalkGrid, HeightGrid) {
        // A 10x10 area with a walkable strip 2 units wide along x at y 4..6.
        let mut walk = WalkGrid::new(Vec2::ZERO, Vec2::splat(10.0));
        for x in 0..10 {
            walk.mark(Vec2::new(x as f32 + 0.5, 4.5), 1.0);
            walk.mark(Vec2::new(x as f32 + 0.5, 5.5), 1.0);
        }
        let mut heights = HeightGrid::new(Vec2::ZERO, [11, 11]);
        for y in 0..11 {
            for x in 0..11 {
                heights.set(x, y, x as f32);
            }
        }
        (walk, heights)
    }

    #[test]
    fn walkable_cells_follow_marks() {
        let (walk, _) = corridor();
        assert!(walk.walkable(Vec2::new(3.2, 4.1)));
        assert!(!walk.walkable(Vec2::new(3.2, 3.9)));
        assert!(!walk.walkable(Vec2::new(-1.0, 5.0)));
    }

    #[test]
    fn heights_interpolate() {
        let (_, heights) = corridor();
        assert_eq!(heights.height(Vec2::new(2.5, 3.0)), 2.5);
        assert_eq!(
            heights.height(Vec2::new(-5.0, 3.0)),
            0.0,
            "clamped at the edge"
        );
    }

    #[test]
    fn slides_along_a_wall_and_follows_the_ground() {
        let (walk, heights) = corridor();
        let mut hero = Hero::new(Vec2::new(2.0, 5.5), &heights);
        // Push diagonally into the wall at y = 6: x keeps moving, y stays.
        for _ in 0..10 {
            hero.step(Vec2::new(1.0, 1.0), WALK_SPEED, 0.05, &walk, &heights);
        }
        assert!(hero.position.x > 3.0, "{:?}", hero.position);
        assert!(hero.position.y < 6.0, "{:?}", hero.position);
        assert!(
            (hero.position.z - hero.position.x).abs() < 1e-4,
            "height follows x"
        );
    }

    #[test]
    fn finds_the_nearest_walkable_cell() {
        let (walk, _) = corridor();
        let p = walk.nearest_walkable(Vec2::new(5.2, 9.0)).unwrap();
        assert!(walk.walkable(p));
        assert!((p.y - 5.75).abs() < 1e-4, "{p}");
    }
}
