//! The Hero walking on a region: movement limited to the navigation's
//! walkable cells. Each navigation layer has its own grid. The navigation does not
//! store heights, so each cell's walking height is chosen among the terrain and
//! the placed meshes' horizontal surfaces by [`assign_surfaces`]: the first layer
//! mostly follows the terrain (and building floors), later ones are bridge decks
//! and upper floors.

use glam::{Vec2, Vec3};

/// Side of a walkability cell in world units: half the smallest navigation leaf,
/// so a cell never skips a stair tread (a 0.5 cell could, rising two risers at once).
pub const CELL: f32 = 0.25;
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

    /// Ledges: neighbouring walkable cells, at least one on a surface, whose
    /// heights differ by more than a step (where the Hero is stopped). Each as
    /// (the cells' midpoint, the lower height, the higher).
    pub fn ledges(&self, ground: &HeightGrid) -> Vec<(Vec2, f32, f32)> {
        let mut out = Vec::new();
        for i in (0..self.cells.len()).filter(|&i| self.cells[i]) {
            let (Some(a), true) = (
                self.height(self.centre(i), ground),
                self.on_surface(self.centre(i)),
            ) else {
                continue;
            };
            for j in self.neighbours(i).filter(|&j| self.cells[j]) {
                if let Some(b) = self.height(self.centre(j), ground)
                    && (a - b).abs() > MAX_STEP
                {
                    out.push(((self.centre(i) + self.centre(j)) / 2.0, a.min(b), a.max(b)));
                }
            }
        }
        out
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
///   not reached whose terrain is covered that way (a floor with no step up to it
///   from open ground, such as a hall raised over the terrain) then grow from
///   that floor, their lowest surface. Cells still not reached follow the terrain.
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
                    // The centre and four points around it, so a seam in a mesh
                    // (a ray slipping between a step's triangles) leaves no hole.
                    let mut c: Vec<f32> = [Vec2::ZERO, Vec2::X, -Vec2::X, Vec2::Y, -Vec2::Y]
                        .iter()
                        .flat_map(|d| surfaces(p + *d * CELL * 0.3))
                        .collect();
                    c.sort_by(f32::total_cmp);
                    c.dedup_by(|a, b| (*a - *b).abs() < 0.01);
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
                let near = [Vec2::ZERO, Vec2::X, -Vec2::X, Vec2::Y, -Vec2::Y].map(|d| p + d);
                let near = near
                    .iter()
                    .chain(
                        [Vec2::X, -Vec2::X, Vec2::Y, -Vec2::Y]
                            .map(|d| p + d * 0.5)
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
        for pass in 0..2 {
            if pass == 1 {
                if l > 0 {
                    break;
                }
                // Floors not reached from open ground: seed from the floor itself.
                for i in 0..n {
                    if !layer.cells[i] || height[i].is_finite() {
                        continue;
                    }
                    let terrain = ground.height(layer.centre(i));
                    let floor = candidates_of(i, &mut candidates)
                        .into_iter()
                        .filter(|&z| z > terrain + 0.05 && z < terrain + HEADROOM)
                        .min_by(f32::total_cmp);
                    if let Some(z) = floor {
                        height[i] = z;
                        queue.push_back(i);
                    }
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

/// A route on foot from `from` (standing on `layer`) to the walkable spot
/// nearest `to`, as world points from start to goal. A* over the walk cells of
/// every layer (all layers share one grid): to the 8 neighbours on a layer (a
/// diagonal only where both sides are walkable, so it does not cut corners),
/// each within a step of the last; and between layers where their surfaces meet
/// within a step. Points `blocked` returns true for (closed doors, people) are
/// avoided; when the goal itself is blocked (a person), the route ends beside
/// it. None if no route reaches the goal.
pub fn route(
    layers: &[WalkGrid],
    ground: &HeightGrid,
    from: Vec3,
    layer: usize,
    to: Vec2,
    blocked: impl Fn(Vec2) -> bool,
) -> Option<Vec<Vec2>> {
    use std::cmp::Ordering;
    use std::collections::BinaryHeap;

    let first = layers.first()?;
    let n = first.cells.len();
    let w = first.size[0] as i64;
    let h = first.size[1] as i64;
    let height = |node: usize| -> Option<f32> {
        let (l, i) = (node / n, node % n);
        let grid = layers.get(l)?;
        if !grid.cells[i] {
            return None;
        }
        grid.height(grid.centre(i), ground)
    };
    let start = layer * n + layers.get(layer)?.index(from.truncate())?;
    height(start)?;
    // The goal: the walkable cell nearest `to` on any layer.
    let goal_cell = first.index(layers.iter().find_map(|g| g.nearest_walkable(to))?)?;
    let goal_point = first.centre(goal_cell);
    // How near the goal counts as there: beside it if it is taken.
    let within = if blocked(goal_point) { 1.2 } else { CELL * 0.5 };

    #[derive(PartialEq)]
    struct Open(f32, usize);
    impl Eq for Open {}
    impl PartialOrd for Open {
        fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
            Some(self.cmp(other))
        }
    }
    impl Ord for Open {
        fn cmp(&self, other: &Self) -> Ordering {
            other.0.total_cmp(&self.0)
        }
    }
    let total = n * layers.len();
    let mut cost = vec![f32::INFINITY; total];
    let mut came = vec![usize::MAX; total];
    let mut open = BinaryHeap::new();
    cost[start] = 0.0;
    open.push(Open(0.0, start));
    let mut reached = None;
    while let Some(Open(_, node)) = open.pop() {
        let (l, i) = (node / n, node % n);
        if first.centre(i).distance(goal_point) <= within {
            reached = Some(node);
            break;
        }
        let z = height(node).unwrap_or(f32::NAN);
        let (x, y) = ((i as i64) % w, (i as i64) / w);
        let mut next: Vec<(usize, f32)> = Vec::with_capacity(8 + layers.len());
        for (dx, dy) in [
            (1, 0),
            (-1, 0),
            (0, 1),
            (0, -1),
            (1, 1),
            (1, -1),
            (-1, 1),
            (-1, -1),
        ] {
            let (nx, ny) = (x + dx, y + dy);
            if nx < 0 || ny < 0 || nx >= w || ny >= h {
                continue;
            }
            let j = (ny * w + nx) as usize;
            let reachable = |k: usize| {
                height(l * n + k).is_some_and(|zk| (zk - z).abs() <= MAX_STEP)
                    && !blocked(first.centre(k))
            };
            if dx != 0
                && dy != 0
                && !(reachable((y * w + nx) as usize) && reachable((ny * w + x) as usize))
            {
                continue;
            }
            if reachable(j) {
                let step = if dx != 0 && dy != 0 {
                    std::f32::consts::SQRT_2
                } else {
                    1.0
                };
                next.push((l * n + j, step * CELL));
            }
        }
        for other in (0..layers.len()).filter(|&o| o != l) {
            if height(other * n + i).is_some_and(|zo| (zo - z).abs() <= MAX_STEP) {
                next.push((other * n + i, 0.01));
            }
        }
        for (m, c) in next {
            let new = cost[node] + c;
            if new < cost[m] {
                cost[m] = new;
                came[m] = node;
                let guess = first.centre(m % n).distance(goal_point);
                open.push(Open(new + guess, m));
            }
        }
    }
    let mut node = reached?;
    let mut points = vec![first.centre(node % n)];
    while came[node] != usize::MAX {
        node = came[node];
        points.push(first.centre(node % n));
    }
    points.reverse();
    Some(points)
}

/// Where `hero` should head along `path` (a [`route`]): drops the points
/// already reached (or passed), at his height, and returns the farthest of the
/// next ones within `ahead` units that he can walk to in a straight line, so
/// the walk cuts the grid's corners without catching on them. None once the
/// path is done.
pub fn steer(
    path: &mut std::collections::VecDeque<Vec2>,
    hero: &Hero,
    layers: &[WalkGrid],
    ground: &HeightGrid,
    ahead: f32,
    blocked: impl Fn(Vec2) -> bool,
) -> Option<Vec2> {
    let at = hero.position.truncate();
    let z = hero.position.z;
    // A point is reached only where the Hero is at its height, not beside it
    // on a lower or higher surface (or below it on a steep climb).
    let level = |q: Vec2| {
        layers
            .iter()
            .filter_map(|g| g.height(q, ground))
            .any(|h| (h - z).abs() <= 0.2)
    };
    while path
        .front()
        .is_some_and(|p| p.distance(at) < 0.3 && level(*p))
        || (path.len() > 1 && path[1].distance(at) < path[0].distance(at) && level(path[1]))
    {
        path.pop_front();
    }
    // Walkable in a straight line: every half cell along the way is free and
    // within a step of the last, on some layer (as `Hero::step_blocked` moves).
    let clear = |to: Vec2| {
        let samples = (at.distance(to) / (CELL * 0.5)).ceil().max(1.0) as usize;
        let mut z = z;
        (1..=samples).all(|k| {
            let q = at.lerp(to, k as f32 / samples as f32);
            if blocked(q) {
                return false;
            }
            let next = layers
                .iter()
                .filter_map(|g| g.height(q, ground))
                .filter(|h| (h - z).abs() <= MAX_STEP)
                .min_by(|a, b| (a - z).abs().total_cmp(&(b - z).abs()));
            next.map(|h| z = h).is_some()
        })
    };
    path.iter()
        .take_while(|p| p.distance(at) <= ahead)
        .filter(|p| clear(**p))
        .last()
        .or(path.front())
        .copied()
}

/// Walks the Hero to a goal along routes, planning a new route from where he is
/// when he makes no progress for a second (someone in the way, a corner).
#[derive(Debug, Clone)]
pub struct Follow {
    pub goal: Vec2,
    path: std::collections::VecDeque<Vec2>,
    planned: bool,
    routed: bool,
    last: Option<Vec2>,
    stuck: f32,
    replans: u32,
}

impl Follow {
    /// Seconds without progress before planning again, and how often to try.
    const STUCK: f32 = 1.0;
    const REPLANS: u32 = 5;

    pub fn new(goal: Vec2) -> Self {
        Self {
            goal,
            path: Default::default(),
            planned: false,
            routed: false,
            last: None,
            stuck: 0.0,
            replans: 0,
        }
    }

    /// The direction to walk this frame (`dt` seconds since the last), or None
    /// once there or given up. `route_blocked` is what routes avoid (with a
    /// margin), `walk_blocked` what stops the Hero.
    #[allow(clippy::too_many_arguments)]
    pub fn direction(
        &mut self,
        hero: &Hero,
        layers: &[WalkGrid],
        ground: &HeightGrid,
        dt: f32,
        route_blocked: impl Fn(Vec2) -> bool,
        walk_blocked: impl Fn(Vec2) -> bool,
    ) -> Option<Vec2> {
        let at = hero.position.truncate();
        if let Some(last) = self.last {
            self.stuck = if last.distance(at) < 0.02 {
                self.stuck + dt
            } else {
                0.0
            };
        }
        self.last = Some(at);
        if self.stuck > Self::STUCK {
            if self.replans >= Self::REPLANS {
                return None;
            }
            self.replans += 1;
            self.planned = false;
            self.stuck = 0.0;
        }
        if !self.planned {
            self.planned = true;
            self.path = route(
                layers,
                ground,
                hero.position,
                hero.layer,
                self.goal,
                &route_blocked,
            )
            .map(Into::into)
            .unwrap_or_default();
            self.routed = !self.path.is_empty();
        }
        if self.routed {
            let next = steer(&mut self.path, hero, layers, ground, 1.5, walk_blocked)?;
            return Some(next - at);
        }
        let to = self.goal - at;
        (to.length() >= 0.3).then_some(to)
    }

    /// Whether a route was found (else the Hero heads straight for the goal).
    pub fn routed(&self) -> bool {
        self.routed
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
    #[cfg(test)]
    pub fn step(
        &mut self,
        direction: Vec2,
        speed: f32,
        dt: f32,
        layers: &[WalkGrid],
        heights: &HeightGrid,
    ) {
        self.step_blocked(direction, speed, dt, layers, heights, |_| false);
    }

    /// As [`Hero::step`], also refusing points `blocked` returns true for (closed
    /// doors, other people).
    pub fn step_blocked(
        &mut self,
        direction: Vec2,
        speed: f32,
        dt: f32,
        layers: &[WalkGrid],
        heights: &HeightGrid,
        blocked: impl Fn(Vec2) -> bool,
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
            .filter(|to| *to != from && !blocked(*to))
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
        // Six 1x1 deck squares.
        assert_eq!(
            layers[1].walkable_cells(),
            (6.0 / (CELL * CELL)) as usize,
            "deck cells all reached"
        );
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
    fn blocked_points_stop_the_step() {
        let (walk, heights) = corridor();
        let mut hero = Hero::new(Vec2::new(2.0, 5.0), &heights);
        // A wall across the corridor at x = 3.
        for _ in 0..20 {
            hero.step_blocked(
                Vec2::X,
                WALK_SPEED,
                0.05,
                std::slice::from_ref(&walk),
                &heights,
                |p| p.x > 3.0,
            );
        }
        assert!(
            hero.position.x <= 3.0 && hero.position.x > 2.5,
            "{:?}",
            hero.position
        );
    }

    #[test]
    fn finds_the_nearest_walkable_cell() {
        let (walk, _) = corridor();
        let p = walk.nearest_walkable(Vec2::new(5.2, 9.0)).unwrap();
        assert!(walk.walkable(p));
        // The centre of the top row of cells in the strip (y 4..6).
        assert!((p.y - (6.0 - CELL / 2.0)).abs() < 1e-4, "{p}");
    }

    #[test]
    fn a_raised_hall_floor_is_walked_on() {
        // Flat ground at 0, walkable along y 4..6. Over x 6..12 a hall floor at
        // 1.3 with no step up to it (higher than a step above the open ground).
        let heights = HeightGrid::new(Vec2::ZERO, [13, 11]);
        let mut ground = WalkGrid::new(Vec2::ZERO, Vec2::new(12.0, 10.0));
        for x in 0..12 {
            ground.mark(Vec2::new(x as f32 + 0.5, 5.0), 1.0);
        }
        let mut layers = [ground];
        assign_surfaces(&mut layers, &heights, |p| {
            if p.x > 6.0 { vec![1.3] } else { Vec::new() }
        });
        let at = |x: f32| layers[0].height(Vec2::new(x, 5.0), &heights);
        assert_eq!(at(2.0), Some(0.0), "open ground");
        assert_eq!(
            at(9.0),
            Some(1.3),
            "the hall floor, not the terrain under it"
        );
    }

    #[test]
    fn routes_go_around_a_wall() {
        // An open 10x10 area with a wall along y = 5 from x 0 to 8: from below it
        // to above it, the route goes round the wall's end at x 8..10.
        let heights = HeightGrid::new(Vec2::ZERO, [11, 11]);
        let mut walk = WalkGrid::new(Vec2::ZERO, Vec2::splat(10.0));
        for y in 0..10 {
            for x in 0..10 {
                if !(y == 5 && x < 8) {
                    walk.mark(Vec2::new(x as f32 + 0.5, y as f32 + 0.5), 1.0);
                }
            }
        }
        let layers = [walk];
        let path = route(
            &layers,
            &heights,
            Vec3::new(2.0, 2.0, 0.0),
            0,
            Vec2::new(2.0, 8.0),
            |_| false,
        )
        .expect("a route round the wall");
        assert!(path.iter().any(|p| p.x > 8.0), "goes round the end");
        assert!(path.last().unwrap().distance(Vec2::new(2.0, 8.0)) < CELL);
        // Someone standing at the wall's end and someone at the goal: round the
        // first, ending beside the second.
        let person = |p: Vec2| {
            p.distance(Vec2::new(9.0, 5.5)) < 0.6 || p.distance(Vec2::new(2.0, 8.0)) < 0.6
        };
        let around = route(
            &layers,
            &heights,
            Vec3::new(2.0, 2.0, 0.0),
            0,
            Vec2::new(2.0, 8.0),
            person,
        )
        .expect("a route round them");
        assert!(around.iter().all(|&p| !person(p)), "never through a person");
        let end = around.last().unwrap().distance(Vec2::new(2.0, 8.0));
        assert!(end > 0.5 && end <= 1.2, "beside the goal: {end}");
        // Walled right across: no route.
        let mut closed = layers[0].clone();
        for x in 0..10 {
            for k in 0..(1.0 / CELL) as usize {
                let i = closed
                    .index(Vec2::new(x as f32 + (k as f32 + 0.5) * CELL, 5.5))
                    .unwrap();
                closed.cells[i] = false;
            }
        }
        let goal = Vec2::new(2.0, 8.0);
        let path = route(
            &[closed],
            &heights,
            Vec3::new(2.0, 2.0, 0.0),
            0,
            goal,
            |_| false,
        );
        assert!(path.is_none(), "no way across");
    }
}
