//! Tangents for bump mapping: per vertex, which way the texture's u runs across
//! the surface, and whether its v runs along the normal crossed with that or
//! against it. Bump maps hold slopes along u and v ([`fable_data::texture::bump_slope`]).

use glam::{Vec2, Vec3};

/// Per vertex, the direction of increasing u (xyz, unit, at right angles to the
/// normal) and the sign of v's direction relative to `normal × tangent` (w), from
/// the triangles that use it. A vertex no triangle with a usable u uses gets a
/// tangent at right angles to its normal.
pub fn tangents(
    positions: &[[f32; 3]],
    normals: &[[f32; 3]],
    uvs: &[[f32; 2]],
    triangles: &[[u32; 3]],
) -> Vec<[f32; 4]> {
    let mut along_u = vec![Vec3::ZERO; positions.len()];
    let mut along_v = vec![Vec3::ZERO; positions.len()];
    for t in triangles {
        let [a, b, c] = t.map(|i| i as usize);
        if [a, b, c].iter().any(|&i| i >= positions.len()) {
            continue;
        }
        let p = |i: usize| Vec3::from_array(positions[i]);
        let q = |i: usize| Vec2::from_array(uvs[i]);
        let (e1, e2) = (p(b) - p(a), p(c) - p(a));
        let (d1, d2) = (q(b) - q(a), q(c) - q(a));
        let det = d1.x * d2.y - d2.x * d1.y;
        if det.abs() < 1e-12 {
            continue;
        }
        let u = (e1 * d2.y - e2 * d1.y) / det;
        let v = (e2 * d1.x - e1 * d2.x) / det;
        for i in [a, b, c] {
            along_u[i] += u;
            along_v[i] += v;
        }
    }
    (0..positions.len())
        .map(|i| {
            let n = Vec3::from_array(normals[i]).normalize_or(Vec3::Z);
            // Gram-Schmidt: u's direction in the surface.
            let mut t = (along_u[i] - n * n.dot(along_u[i])).normalize_or_zero();
            if t == Vec3::ZERO {
                t = n.any_orthonormal_vector();
            }
            let w = if n.cross(t).dot(along_v[i]) < 0.0 {
                -1.0
            } else {
                1.0
            };
            [t.x, t.y, t.z, w]
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Positions, normals, UVs and triangles.
    type Mesh = (Vec<[f32; 3]>, Vec<[f32; 3]>, Vec<[f32; 2]>, Vec<[u32; 3]>);

    /// A unit quad in the xy plane facing +z, u along +x and v along -y (rows
    /// down the texture as y falls).
    fn quad() -> Mesh {
        (
            vec![
                [0.0, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                [1.0, 1.0, 0.0],
                [0.0, 1.0, 0.0],
            ],
            vec![[0.0, 0.0, 1.0]; 4],
            vec![[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]],
            vec![[0, 1, 2], [0, 2, 3]],
        )
    }

    #[test]
    fn the_tangent_follows_u_and_its_sign_v() {
        let (p, n, uv, t) = quad();
        for tangent in tangents(&p, &n, &uv, &t) {
            assert_eq!(tangent[..3], [1.0, 0.0, 0.0]);
            // n × t = +y, but v runs along -y.
            assert_eq!(tangent[3], -1.0);
        }
    }

    #[test]
    fn a_mirrored_texture_flips_the_sign() {
        let (p, n, mut uv, t) = quad();
        for c in &mut uv {
            c[1] = 1.0 - c[1];
        }
        assert!(tangents(&p, &n, &uv, &t).iter().all(|t| t[3] == 1.0));
    }

    #[test]
    fn a_vertex_without_texture_space_still_gets_a_tangent() {
        let (p, n, _, t) = quad();
        let flat = vec![[0.5, 0.5]; 4];
        for tangent in tangents(&p, &n, &flat, &t) {
            let v = Vec3::new(tangent[0], tangent[1], tangent[2]);
            assert!((v.length() - 1.0).abs() < 1e-5 && v.z.abs() < 1e-5);
        }
    }
}
