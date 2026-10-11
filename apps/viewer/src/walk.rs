//! The Hero walking on a region: movement limited to the navigation's
//! walkable cells. Each navigation layer has its own grid. The navigation does not
//! store heights, so each cell's walking height is chosen among the terrain and
//! the placed meshes' horizontal surfaces by [`assign_surfaces`]: the first layer
//! mostly follows the terrain (and building floors), later ones are bridge decks
//! and upper floors.

use glam::{Vec2, Vec3};

/// Side of a walkability cell in world units (the smallest navigation leaf).
pub const CELL: f32 = 0.5;
/// Walking speed in world units (metres) per second.
pub const WALK_SPEED: f32 = 4.0;
pub const RUN_SPEED: f32 = 8.0;
/// Least clearance above the terrain to stand on it; terrain covered lower than
/// this (by a deck or a floor) is not walked on.
pub const HEADROOM: f32 = 1.6;
/// Largest height change in one step, up or down; also how far apart two layers'
/// surfaces may be where the Hero crosses from one to the other.
pub const MAX_STEP: f32 = 0.6;

/// Which 0.5 m cells of a region are walkable.
#[derive(Debug, Clone)]
pub struct WalkGrid {
    origin: Vec2,
    size: [u32; 2],
    cells: Vec<bool>,
    /// Per cell, the walking surface height when it is not the terrain's (NaN
    /// where it is); `None` until [`assign_surfaces`] runs.
    surface: Option<Vec<f32>>,
    /// The ground layer: cells without a surface height follow the terrain.
    on_terrain: bool,
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
            surface: None,
            on_terrain: true,
        }
    }

    /// Number of walkable cells.
    pub fn walkable_cells(&self) -> usize {
        self.cells.iter().filter(|&&c| c).count()
    }

    /// Centres of the walkable cells.
    pub fn cell_centres(&self) -> impl Iterator<Item = Vec2> + '_ {
        (0..self.cells.len())
            .filter(|&i| self.cells[i])
            .map(|i| self.centre(i))
    }

    /// Whether `p` has a surface height of its own (a mesh floor or deck) rather
    /// than following the terrain.
    pub fn on_surface(&self, p: Vec2) -> bool {
        self.index(p)
            .and_then(|i| self.surface.as_ref().map(|s| s[i].is_finite()))
            .unwrap_or(false)
    }

    fn centre(&self, i: usize) -> Vec2 {
        let (x, y) = (i as u32 % self.size[0], i as u32 / self.size[0]);
        self.origin + (Vec2::new(x as f32, y as f32) + 0.5) * CELL
    }

    /// The 4 neighbouring cell indices of cell `i`.
    fn neighbours(&self, i: usize) -> impl Iterator<Item = usize> {
        let (w, h) = (self.size[0] as usize, self.size[1] as usize);
        let (x, y) = (i % w, i / w);
        [
            (x > 0).then(|| i - 1),
            (x + 1 < w).then(|| i + 1),
            (y > 0).then(|| i - w),
            (y + 1 < h).then(|| i + w),
        ]
        .into_iter()
        .flatten()
    }

    /// The walking height at `p`, if `p` is walkable on this layer.
    pub fn height(&self, p: Vec2, ground: &HeightGrid) -> Option<f32> {
        let i = self.index(p)?;
        if !self.cells[i] {
            return None;
        }
        match self.surface.as_ref().map(|s| s[i]) {
            Some(h) if h.is_finite() => Some(h),
            _ if self.on_terrain => Some(ground.height(p)),
            _ => None,
        }
    }

    fn index(&self, p: Vec2) -> Option<usize> {
        let c = ((p - self.origin) / CELL).floor();
        (c.x >= 0.0 && c.y >= 0.0 && (c.x as u32) < self.size[0] && (c.y as u32) < self.size[1])
            .then(|| (c.y as u32 * self.size[0] + c.x as u32) as usize)
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
        self.index(p).is_some_and(|i| self.cells[i])
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

/// Chooses every cell's walking height, layer by layer, by growing out from cells
/// whose height is known, so that neighbouring cells differ by at most
/// [`MAX_STEP`]. `surfaces(p)` lists the heights of the placed meshes' horizontal
/// surfaces above or below `p` (floors, decks, but also roofs and table tops).
///
/// - Ground layer: grows from cells with no surface between the terrain and 3 units
///   above it (open ground), where the terrain is the height. Each further cell
///   takes whichever of the terrain and its surfaces is closest to the neighbour it
///   is reached from, the highest within a step of it: the Hero stands on top
///   of things, so the layer climbs ramps and steps onto floors and decks. The
///   terrain is not a choice where a surface covers it within [`HEADROOM`]. Cells
///   not reached follow the terrain.
/// - Later layers (all `layers` must share one grid): grow from cells with a surface
///   within a step of the walking height on an earlier layer at the same spot or up
///   to 1 unit away (the ends of a bridge deck, a doorway). Cells not reached are
///   unwalkable.
pub fn assign_surfaces(
    layers: &mut [WalkGrid],
    ground: &HeightGrid,
    surfaces: impl Fn(Vec2) -> Vec<f32>,
) {
    for l in 0..layers.len() {
        let (below, rest) = layers.split_at_mut(l);
        let layer = &mut rest[0];
        layer.on_terrain = l == 0;
        let n = layer.cells.len();
        let mut height = vec![f32::NAN; n];
        let mut candidates: Vec<Option<Vec<f32>>> = vec![None; n];
        let mut queue = std::collections::VecDeque::new();
        let candidates_of = |i: usize, candidates: &mut Vec<Option<Vec<f32>>>| -> Vec<f32> {
            candidates[i]
                .get_or_insert_with(|| {
                    let p = layer.centre(i);
                    let mut c = surfaces(p);
                    // The terrain counts unless something covers it too low to
                    // stand under (a deck or floor laid over it).
                    let terrain = ground.height(p);
                    if l == 0
                        && !c
                            .iter()
                            .any(|&z| z > terrain + 0.05 && z < terrain + HEADROOM)
                    {
                        c.push(terrain);
                    }
                    c
                })
                .clone()
        };
        for i in (0..n).filter(|&i| layer.cells[i]) {
            let p = layer.centre(i);
            let seed = if l == 0 {
                let terrain = ground.height(p);
                let floors = candidates_of(i, &mut candidates);
                (!floors
                    .iter()
                    .any(|&z| z > terrain + 0.05 && z < terrain + 3.0))
                .then_some(terrain)
            } else {
                // The same spot or up to 1 unit away on an earlier layer: layers
                // meet side by side at doorways rather than overlapping.
                let near =
                    [Vec2::ZERO, Vec2::X, -Vec2::X, Vec2::Y, -Vec2::Y].map(|d| p + d * 2.0 * CELL);
                let near = near
                    .iter()
                    .chain(
                        [Vec2::X, -Vec2::X, Vec2::Y, -Vec2::Y]
                            .map(|d| p + d * CELL)
                            .iter(),
                    )
                    .copied()
                    .collect::<Vec<_>>();
                below
                    .iter()
                    .flat_map(|b| near.iter().filter_map(|&q| b.height(q, ground)))
                    .find_map(|z| {
                        candidates_of(i, &mut candidates)
                            .into_iter()
                            .filter(|c| (c - z).abs() <= MAX_STEP)
                            .max_by(f32::total_cmp)
                    })
            };
            if let Some(h) = seed {
                height[i] = h;
                queue.push_back(i);
            }
        }
        while let Some(i) = queue.pop_front() {
            let h = height[i];
            for j in layer.neighbours(i).collect::<Vec<_>>() {
                if !layer.cells[j] || height[j].is_finite() {
                    continue;
                }
                if let Some(z) = candidates_of(j, &mut candidates)
                    .into_iter()
                    .filter(|c| (c - h).abs() <= MAX_STEP)
                    .max_by(f32::total_cmp)
                {
                    height[j] = z;
                    queue.push_back(j);
                }
            }
        }
        if l > 0 {
            for i in 0..n {
                layer.cells[i] &= height[i].is_finite();
            }
        } else {
            // Cells on the terrain store NaN, so they keep its bilinear smoothness.
            for (i, h) in height.iter_mut().enumerate() {
                if h.is_finite() && (*h - ground.height(layer.centre(i))).abs() < 1e-3 {
                    *h = f32::NAN;
                }
            }
        }
        layer.surface = Some(height);
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
    /// The navigation layer the Hero is on (0 = the ground).
    pub layer: usize,
}

impl Hero {
    pub fn new(at: Vec2, heights: &HeightGrid) -> Self {
        Self {
            position: at.extend(heights.height(at)),
            heading: 0.0,
            layer: 0,
        }
    }

    /// Moves along `direction` (world x/y, any length) at `speed` for `dt` seconds,
    /// staying on walkable cells: if the full step is blocked, tries each axis alone
    /// so the Hero slides along edges. A step may land on any layer whose surface
    /// there is within [`MAX_STEP`] of the Hero's height, the closest first (the
    /// current layer on a tie), which is how he gets on and off a bridge deck.
    pub fn step(
        &mut self,
        direction: Vec2,
        speed: f32,
        dt: f32,
        layers: &[WalkGrid],
        heights: &HeightGrid,
    ) {
        let Some(dir) = direction.try_normalize() else {
            return;
        };
        let from = self.position.truncate();
        let delta = dir * speed * dt;
        let candidates = [delta, Vec2::new(delta.x, 0.0), Vec2::new(0.0, delta.y)];
        for to in candidates
            .iter()
            .map(|d| from + *d)
            .filter(|to| *to != from)
        {
            let best = layers
                .iter()
                .enumerate()
                .filter_map(|(i, layer)| {
                    let z = layer.height(to, heights)?;
                    let rise = (z - self.position.z).abs();
                    (rise <= MAX_STEP).then_some((rise, i != self.layer, i, z))
                })
                .min_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
            if let Some((_, _, layer, z)) = best {
                self.position = to.extend(z);
                self.layer = layer;
                break;
            }
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
            hero.step(
                Vec2::new(1.0, 1.0),
                WALK_SPEED,
                0.05,
                std::slice::from_ref(&walk),
                &heights,
            );
        }
        assert!(hero.position.x > 3.0, "{:?}", hero.position);
        assert!(hero.position.y < 6.0, "{:?}", hero.position);
        assert!(
            (hero.position.z - hero.position.x).abs() < 1e-4,
            "height follows x"
        );
    }

    #[test]
    fn crosses_a_bridge_deck_over_a_ravine() {
        // Plateaus at height 5 for x < 4 and x > 8, a ravine at height 0 between;
        // the ground is walkable everywhere along y 4..6. A deck layer spans x 3..9,
        // 0.4 above the plateaus, with a ramp up to it on the ground layer.
        let mut heights = HeightGrid::new(Vec2::ZERO, [13, 11]);
        for y in 0..11 {
            for x in 0..13 {
                heights.set(x, y, if (4..=8).contains(&x) { 0.0 } else { 5.0 });
            }
        }
        let mut ground = WalkGrid::new(Vec2::ZERO, Vec2::new(12.0, 10.0));
        let mut deck = WalkGrid::new(Vec2::ZERO, Vec2::new(12.0, 10.0));
        for x in 0..12 {
            ground.mark(Vec2::new(x as f32 + 0.5, 5.0), 1.0);
            if (3..9).contains(&x) {
                deck.mark(Vec2::new(x as f32 + 0.5, 5.0), 1.0);
            }
        }
        let mut layers = [ground, deck];
        // The deck mesh: a top at 5.4 over x 3..9, reached by a ramp over x 2..3
        // (one surface per 0.5 cell, 5.1 then 5.3).
        assign_surfaces(&mut layers, &heights, |p| match p.x {
            x if (2.0..2.5).contains(&x) => vec![5.1],
            x if (2.5..3.0).contains(&x) => vec![5.3],
            x if (3.0..9.0).contains(&x) => vec![5.4],
            _ => vec![],
        });
        assert_eq!(layers[1].walkable_cells(), 24, "deck cells all reached");
        let mut hero = Hero::new(Vec2::new(1.0, 5.0), &heights);
        let mut highest_in_ravine = f32::NEG_INFINITY;
        for _ in 0..60 {
            hero.step(Vec2::X, WALK_SPEED, 0.05, &layers, &heights);
            if (5.0..7.0).contains(&hero.position.x) {
                highest_in_ravine = highest_in_ravine.max(hero.position.z);
                assert_eq!(hero.layer, 1, "{:?}", hero.position);
            }
        }
        assert!(hero.position.x > 10.0, "{:?}", hero.position);
        assert_eq!(highest_in_ravine, 5.4);
        assert_eq!(hero.layer, 0, "back on the far plateau");
    }

    #[test]
    fn finds_the_nearest_walkable_cell() {
        let (walk, _) = corridor();
        let p = walk.nearest_walkable(Vec2::new(5.2, 9.0)).unwrap();
        assert!(walk.walkable(p));
        assert!((p.y - 5.75).abs() < 1e-4, "{p}");
    }
}
