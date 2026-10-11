//! Ray casts against a region's placed meshes and ground, for the follow camera.

use glam::{Vec2, Vec3};

use crate::walk::HeightGrid;

/// Side of a bucket of triangles, in world units.
const CELL: f32 = 4.0;

/// World-space triangles bucketed on a 2D grid by their x/y bounds.
pub struct Colliders {
    origin: Vec2,
    size: [u32; 2],
    cells: Vec<Vec<u32>>,
    triangles: Vec<[Vec3; 3]>,
}

impl Colliders {
    pub fn new(triangles: Vec<[Vec3; 3]>) -> Self {
        let (mut lo, mut hi) = (Vec2::splat(f32::INFINITY), Vec2::splat(f32::NEG_INFINITY));
        for p in triangles.iter().flatten() {
            lo = lo.min(p.truncate());
            hi = hi.max(p.truncate());
        }
        if triangles.is_empty() {
            (lo, hi) = (Vec2::ZERO, Vec2::ZERO);
        }
        let size = [
            ((hi.x - lo.x) / CELL).floor() as u32 + 1,
            ((hi.y - lo.y) / CELL).floor() as u32 + 1,
        ];
        let mut colliders = Self {
            origin: lo,
            size,
            cells: vec![Vec::new(); size[0] as usize * size[1] as usize],
            triangles: Vec::new(),
        };
        for (i, t) in triangles.iter().enumerate() {
            let tlo = t[0].truncate().min(t[1].truncate()).min(t[2].truncate());
            let thi = t[0].truncate().max(t[1].truncate()).max(t[2].truncate());
            let (c0, c1) = (colliders.cell(tlo), colliders.cell(thi));
            for y in c0[1]..=c1[1] {
                for x in c0[0]..=c1[0] {
                    colliders.cells[(y * size[0] + x) as usize].push(i as u32);
                }
            }
        }
        colliders.triangles = triangles;
        colliders
    }

    /// The grid cell holding `p`, clamped to the grid.
    fn cell(&self, p: Vec2) -> [u32; 2] {
        let c = ((p - self.origin) / CELL).floor().max(Vec2::ZERO);
        [
            (c.x as u32).min(self.size[0] - 1),
            (c.y as u32).min(self.size[1] - 1),
        ]
    }

    /// Distance along `dir` (unit length) from `from` to the nearest triangle
    /// within `max`, from either side.
    pub fn ray(&self, from: Vec3, dir: Vec3, max: f32) -> Option<f32> {
        let to = from + dir * max;
        let (c0, c1) = (
            self.cell(from.truncate().min(to.truncate())),
            self.cell(from.truncate().max(to.truncate())),
        );
        let mut best: Option<f32> = None;
        for y in c0[1]..=c1[1] {
            for x in c0[0]..=c1[0] {
                for &i in &self.cells[(y * self.size[0] + x) as usize] {
                    if let Some(t) = hit_triangle(from, dir, &self.triangles[i as usize])
                        && t <= max
                        && best.is_none_or(|b| t < b)
                    {
                        best = Some(t);
                    }
                }
            }
        }
        best
    }
}

/// Möller–Trumbore, double-sided: the distance along the ray, if it hits.
fn hit_triangle(from: Vec3, dir: Vec3, t: &[Vec3; 3]) -> Option<f32> {
    let (e1, e2) = (t[1] - t[0], t[2] - t[0]);
    let p = dir.cross(e2);
    let det = e1.dot(p);
    if det.abs() < 1e-9 {
        return None;
    }
    let s = from - t[0];
    let u = s.dot(p) / det;
    if !(0.0..=1.0).contains(&u) {
        return None;
    }
    let q = s.cross(e1);
    let v = dir.dot(q) / det;
    if v < 0.0 || u + v > 1.0 {
        return None;
    }
    let d = e2.dot(q) / det;
    (d > 0.0).then_some(d)
}

/// Distance along `dir` from `from` to where the ray first goes below the
/// ground plus `clearance`, marching in 0.25-unit steps.
pub fn ground_ray(
    heights: &HeightGrid,
    from: Vec3,
    dir: Vec3,
    max: f32,
    clearance: f32,
) -> Option<f32> {
    let steps = (max / 0.25).ceil() as u32;
    (1..=steps).map(|k| (k as f32 * 0.25).min(max)).find(|&d| {
        let p = from + dir * d;
        p.z < heights.height(p.truncate()) + clearance
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hits_the_nearest_wall_from_either_side() {
        // Two walls across the x axis, at x = 3 and x = 6.
        let wall = |x: f32| {
            [
                [
                    Vec3::new(x, -5.0, -5.0),
                    Vec3::new(x, 5.0, -5.0),
                    Vec3::new(x, 0.0, 5.0),
                ],
                [
                    Vec3::new(x, 5.0, -5.0),
                    Vec3::new(x, -5.0, -5.0),
                    Vec3::new(x, 0.0, 5.0),
                ],
            ]
        };
        let colliders = Colliders::new(wall(3.0).into_iter().chain(wall(6.0)).collect());
        let d = colliders.ray(Vec3::ZERO, Vec3::X, 10.0).unwrap();
        assert!((d - 3.0).abs() < 1e-5, "{d}");
        let back = colliders
            .ray(Vec3::new(10.0, 0.0, 0.0), Vec3::NEG_X, 10.0)
            .unwrap();
        assert!((back - 4.0).abs() < 1e-5, "{back}");
        assert_eq!(colliders.ray(Vec3::ZERO, Vec3::X, 2.0), None, "beyond max");
        assert_eq!(colliders.ray(Vec3::ZERO, Vec3::Y, 10.0), None, "parallel");
    }

    #[test]
    fn ground_ray_stops_at_rising_ground() {
        // Ground rising 1 unit per unit along x.
        let mut heights = HeightGrid::new(Vec2::ZERO, [21, 21]);
        for y in 0..21 {
            for x in 0..21 {
                heights.set(x, y, x as f32);
            }
        }
        let d = ground_ray(&heights, Vec3::new(1.0, 5.0, 3.0), Vec3::X, 10.0, 0.0).unwrap();
        assert!((d - 2.0).abs() <= 0.25, "{d}");
        assert_eq!(
            ground_ray(&heights, Vec3::new(1.0, 5.0, 3.0), Vec3::NEG_X, 1.0, 0.0),
            None
        );
    }
}
