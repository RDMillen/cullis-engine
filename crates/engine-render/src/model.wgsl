struct Uniforms {
    view_proj: mat4x4<f32>,
    // xyz: direction towards the sun.
    sun: vec4<f32>,
    // xyz: the camera, in the model's own space (for the highlight's rim).
    eye: vec4<f32>,
    // rgb: highlight colour; a: its strength (0: none).
    highlight: vec4<f32>,
    // rgb: the sun's and the ambient light's colours (gamma space); ambient.a:
    // brightness.
    sun_colour: vec4<f32>,
    ambient: vec4<f32>,
    // The model's (or world's) position to the sun's clip space, for shadows.
    shadow_proj: mat4x4<f32>,
    // x: shadow strength (0: none); y: one shadow map texel in uv; z: depth bias.
    shadow: vec4<f32>,
    // The model's own space to the world (for point lights).
    model: mat4x4<f32>,
    // Point lights, two entries each (see lighting.rs), and how many (x).
    points: array<vec4<f32>, 32>,
    point_count: vec4<f32>,
};

@group(0) @binding(0) var<uniform> u: Uniforms;
@group(1) @binding(0) var tex: texture_2d<f32>;
@group(1) @binding(1) var samp: sampler;
@group(2) @binding(0) var shadow_map: texture_depth_2d;
@group(2) @binding(1) var shadow_samp: sampler_comparison;

struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) normal: vec3<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) local: vec3<f32>,
    @location(3) shadow_pos: vec4<f32>,
    @location(4) world: vec3<f32>,
    @location(5) world_normal: vec3<f32>,
};

@vertex
fn vs_main(
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
) -> VsOut {
    var out: VsOut;
    out.clip = u.view_proj * vec4<f32>(position, 1.0);
    out.normal = normal;
    out.uv = uv;
    out.local = position;
    out.shadow_pos = u.shadow_proj * vec4<f32>(position, 1.0);
    out.world = (u.model * vec4<f32>(position, 1.0)).xyz;
    out.world_normal = (u.model * vec4<f32>(normal, 0.0)).xyz;
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


// Light from the sun and the ambient light, added in gamma space and clamped as
// old fixed-function lighting did, then made linear for the sRGB target.
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

// Placeholder material: the main texture, alpha-tested, lit by the sun.
@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let c = textureSample(tex, samp, in.uv);
    if c.a < 0.5 {
        discard;
    }
    let n = normalize(in.normal);
    let points = points_at(in.world, normalize(in.world_normal));
    var rgb = c.rgb * lit(n, sunlit(in.shadow_pos), points);
    // The interaction highlight: a glow at the silhouette's edge and a faint tint.
    if u.highlight.a > 0.0 {
        let v = normalize(u.eye.xyz - in.local);
        let rim = pow(1.0 - abs(dot(n, v)), 2.5);
        rgb = rgb + u.highlight.rgb * (rim * 0.9 + 0.03) * u.highlight.a;
    }
    return vec4<f32>(rgb, 1.0);
}

// The shadow pass: depth only, with the same alpha test (leaves, fringes).
@fragment
fn fs_shadow(in: VsOut) {
    if textureSample(tex, samp, in.uv).a < 0.5 {
        discard;
    }
}
