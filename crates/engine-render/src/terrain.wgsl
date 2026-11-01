struct Uniforms {
    view_proj: mat4x4<f32>,
    // xyz: direction towards the sun.
    sun: vec4<f32>,
    // Theme grid origin (xy) and size (zw) in world units.
    grid: vec4<f32>,
    // x: world units per texture repeat.
    params: vec4<f32>,
    // rgb: the sun's and the ambient light's colours (gamma space); ambient.a:
    // brightness.
    sun_colour: vec4<f32>,
    ambient: vec4<f32>,
    // The model's (or world's) position to the sun's clip space, for shadows.
    shadow_proj: mat4x4<f32>,
    // x: shadow strength (0: none); y: one shadow map texel in uv; z: depth bias.
    shadow: vec4<f32>,
    // Point lights, two entries each (see lighting.rs), and how many (x).
    points: array<vec4<f32>, 32>,
    point_count: vec4<f32>,
};

@group(0) @binding(0) var<uniform> u: Uniforms;
// Per grid point: first layer, second layer, first weight, second weight.
@group(0) @binding(1) var theme_grid: texture_2d<u32>;
@group(0) @binding(2) var base_tex: texture_2d_array<f32>;
@group(0) @binding(3) var cliff_tex: texture_2d_array<f32>;
@group(0) @binding(4) var samp: sampler;
// The themes' bump maps, ground and cliff: slopes along u and v in red and
// green, centred on 128 (flat for themes without one).
@group(0) @binding(5) var base_bump: texture_2d_array<f32>;
@group(0) @binding(6) var cliff_bump: texture_2d_array<f32>;

// How strongly bump maps tilt the ground (as model.wgsl's, by eye).
const BUMP_STRENGTH: f32 = 1.0;
@group(1) @binding(0) var shadow_map: texture_depth_2d;
@group(1) @binding(1) var shadow_samp: sampler_comparison;

struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) normal: vec3<f32>,
    @location(1) world: vec3<f32>,
    @location(2) shadow_pos: vec4<f32>,
};

@vertex
fn vs_main(@location(0) position: vec3<f32>, @location(1) normal: vec3<f32>) -> VsOut {
    var out: VsOut;
    out.clip = u.view_proj * vec4<f32>(position, 1.0);
    out.normal = normal;
    out.world = position;
    out.shadow_pos = u.shadow_proj * vec4<f32>(position, 1.0);
    return out;
}

// Light from the point lights (lamps, fires) at a world position with a world
// normal: full within the inner radius, fading to nothing at the outer.
fn points_at(world: vec3<f32>, n: vec3<f32>) -> vec3<f32> {
    var sum = vec3<f32>(0.0);
    let count = i32(u.point_count.x);
    for (var k = 0; k < count; k++) {
        let at = u.points[2 * k];
        let colour = u.points[2 * k + 1];
        let to = at.xyz - world;
        let d = length(to);
        let reach = clamp((at.w - d) / max(at.w - colour.w, 0.01), 0.0, 1.0);
        let facing = max(dot(n, to / max(d, 0.001)), 0.0);
        sum += colour.rgb * reach * facing;
    }
    return sum;
}


// How much of the sun reaches a point (0..1), from its position in the sun's
// clip space: a 3x3 comparison of the shadow map, faded by the strength.
fn sunlit(pos: vec4<f32>) -> f32 {
    if u.shadow.x <= 0.0 {
        return 1.0;
    }
    let p = pos.xyz / pos.w;
    let uv = vec2<f32>(p.x * 0.5 + 0.5, 0.5 - p.y * 0.5);
    if any(uv < vec2<f32>(0.0)) || any(uv > vec2<f32>(1.0)) || p.z > 1.0 {
        return 1.0;
    }
    let texel = u.shadow.y;
    var seen = 0.0;
    for (var y = -1; y <= 1; y++) {
        for (var x = -1; x <= 1; x++) {
            let at = uv + vec2<f32>(f32(x), f32(y)) * texel;
            seen += textureSampleCompareLevel(shadow_map, shadow_samp, at, p.z - u.shadow.z);
        }
    }
    return mix(1.0, seen / 9.0, u.shadow.x);
}


struct Blend {
    base: vec3<f32>,
    cliff: vec3<f32>,
    // Slopes along u and v, weighted like the colours.
    base_slope: vec2<f32>,
    cliff_slope: vec2<f32>,
};

// A bump map texel's slope (-1 to 1 along u and v).
fn slope(texel: vec4<f32>) -> vec2<f32> {
    return (texel.rg * 255.0 - 128.0) / 127.0;
}

// Adds one grid point's two themes, weighted, to the blend.
fn add_point(acc: Blend, p: vec2<i32>, weight: f32, uv: vec2<f32>, cliff_uv: vec2<f32>) -> Blend {
    let size = vec2<i32>(u.grid.zw);
    let c = textureLoad(theme_grid, clamp(p, vec2<i32>(0), size - 1), 0);
    let w0 = weight * f32(c.z) / 255.0;
    let w1 = weight * f32(c.w) / 255.0;
    var out = acc;
    out.base += w0 * textureSample(base_tex, samp, uv, i32(c.x)).rgb
              + w1 * textureSample(base_tex, samp, uv, i32(c.y)).rgb;
    out.cliff += w0 * textureSample(cliff_tex, samp, cliff_uv, i32(c.x)).rgb
               + w1 * textureSample(cliff_tex, samp, cliff_uv, i32(c.y)).rgb;
    out.base_slope += w0 * slope(textureSample(base_bump, samp, uv, i32(c.x)))
                    + w1 * slope(textureSample(base_bump, samp, uv, i32(c.y)));
    out.cliff_slope += w0 * slope(textureSample(cliff_bump, samp, cliff_uv, i32(c.x)))
                     + w1 * slope(textureSample(cliff_bump, samp, cliff_uv, i32(c.y)));
    return out;
}

// A normal tilted by slope `s` along the surface directions of the texture's
// u and v (`u_dir`, `v_dir`: the world axes its coordinates come from): the
// surface rises towards +u and +v, so the normal leans back from them.
// A direction's part along the surface, unit length, or zero where it points
// along the normal (the ground's axes on a cliff face, a cliff's up on flat
// ground; those blends do not show there, but must not be NaN).
fn in_surface(n: vec3<f32>, d: vec3<f32>) -> vec3<f32> {
    let along = d - n * dot(n, d);
    let len = length(along);
    return select(vec3<f32>(0.0), along / len, len > 1e-4);
}

fn bumped(n: vec3<f32>, u_dir: vec3<f32>, v_dir: vec3<f32>, s: vec2<f32>) -> vec3<f32> {
    let along_u = in_surface(n, u_dir);
    let along_v = in_surface(n, v_dir);
    return normalize(n - BUMP_STRENGTH * (s.x * along_u + s.y * along_v));
}

// Light from the sun and the ambient light, as in model.wgsl.
fn lit(n: vec3<f32>, sun_seen: f32, points: vec3<f32>) -> vec3<f32> {
    let ndl = max(dot(n, normalize(u.sun.xyz)), 0.0) * sun_seen;
    let sum = clamp(
        u.ambient.rgb + u.sun_colour.rgb * ndl + points,
        vec3<f32>(0.0),
        vec3<f32>(1.0),
    );
    // ambient.a: brightness, letting light go past 1 (overbright); sun_colour.a:
    // the most light may reach, keeping its colour, so sunlit faces do not
    // wash out.
    var l = sum * u.ambient.a;
    let top = max(l.r, max(l.g, l.b));
    if top > u.sun_colour.a {
        l = l * (u.sun_colour.a / top);
    }
    return pow(l, vec3<f32>(2.2));
}

// Theme textures blended bilinearly between grid points, ground fading to cliff on
// steep slopes, lit by the sun.
@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let n = normalize(in.normal);
    let g = in.world.xy - u.grid.xy;
    let g0 = floor(g);
    let f = g - g0;
    let p = vec2<i32>(g0);
    let uv = in.world.xy / u.params.x;
    // Cliffs are projected onto the vertical plane the slope faces most.
    var cliff_uv = in.world.yz / u.params.x;
    var cliff_u = vec3<f32>(0.0, 1.0, 0.0);
    if abs(n.y) > abs(n.x) {
        cliff_uv = in.world.xz / u.params.x;
        cliff_u = vec3<f32>(1.0, 0.0, 0.0);
    }
    var b = Blend(vec3<f32>(0.0), vec3<f32>(0.0), vec2<f32>(0.0), vec2<f32>(0.0));
    b = add_point(b, p, (1.0 - f.x) * (1.0 - f.y), uv, cliff_uv);
    b = add_point(b, p + vec2<i32>(1, 0), f.x * (1.0 - f.y), uv, cliff_uv);
    b = add_point(b, p + vec2<i32>(0, 1), (1.0 - f.x) * f.y, uv, cliff_uv);
    b = add_point(b, p + vec2<i32>(1, 1), f.x * f.y, uv, cliff_uv);
    let ground = smoothstep(0.55, 0.8, n.z);
    let albedo = mix(b.cliff, b.base, ground);
    // Ground textures run along world x and y, cliffs along their plane and up.
    let ground_n = bumped(n, vec3<f32>(1.0, 0.0, 0.0), vec3<f32>(0.0, 1.0, 0.0), b.base_slope);
    let cliff_n = bumped(n, cliff_u, vec3<f32>(0.0, 0.0, 1.0), b.cliff_slope);
    let lit_n = normalize(mix(cliff_n, ground_n, ground));
    return vec4<f32>(albedo * lit(lit_n, sunlit(in.shadow_pos), points_at(in.world, lit_n)), 1.0);
}

// Flat placeholder sea: translucent, slightly lit so it reads as a surface.
@fragment
fn fs_water(in: VsOut) -> @location(0) vec4<f32> {
    let light = lit(normalize(in.normal), sunlit(in.shadow_pos), vec3<f32>(0.0)) * 0.6 + pow(u.ambient.rgb, vec3<f32>(2.2)) * 0.4;
    return vec4<f32>(vec3<f32>(0.10, 0.32, 0.45) * light * 1.6, 0.8);
}

// Navigation overlay: flat colour carried in the normal attribute, translucent.
@fragment
fn fs_overlay(in: VsOut) -> @location(0) vec4<f32> {
    return vec4<f32>(in.normal, 0.45);
}
