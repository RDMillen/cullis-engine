// The sky: the environment's sky textures, blended by weight and tinted, on a
// full-screen triangle behind everything. Each texture is a panorama, the
// zenith at its top and the horizon at its bottom, centred on the sun. Two
// cloud layers drift over it, projected onto a plane overhead.

struct Uniforms {
    inv_view_proj: mat4x4<f32>,
    // xyz: the camera.
    eye: vec4<f32>,
    // rgb: tint (linear); a: brightness (gamma space, as the scene's light).
    tint: vec4<f32>,
    // Weight of each array layer (layers 0-3, then 4-7).
    weights: array<vec4<f32>, 2>,
    // xyz: towards the sun.
    sun: vec4<f32>,
    // The cloud layers' array layers (x, y; -1 for none) and opacities (z, w).
    clouds: vec4<f32>,
    // rgb: the clouds' colour (linear); a: seconds, for drifting.
    cloud_colour: vec4<f32>,
};

@group(0) @binding(0) var<uniform> u: Uniforms;
@group(0) @binding(1) var skies: texture_2d_array<f32>;
@group(0) @binding(2) var samp: sampler;

struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) ndc: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) i: u32) -> VsOut {
    let p = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u)) * 2.0 - 1.0;
    var out: VsOut;
    out.clip = vec4<f32>(p, 0.0, 1.0);
    out.ndc = p;
    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let far = u.inv_view_proj * vec4<f32>(in.ndc, 0.5, 1.0);
    let dir = normalize(far.xyz / far.w - u.eye.xyz);
    let elevation = asin(clamp(dir.z, -1.0, 1.0));
    // Below the horizon, the horizon's colour (not quite the bottom row, whose
    // texels hold a dark edge in some skies).
    let v = clamp(1.0 - elevation / 1.5707964, 0.01, 0.97);
    let azimuth = atan2(dir.y, dir.x) - atan2(u.sun.y, u.sun.x);
    let uv = vec2<f32>(0.5 + azimuth / 6.2831853, v);
    var c = vec3<f32>(0.0);
    for (var k = 0; k < 8; k++) {
        c += textureSampleLevel(skies, samp, uv, k, 0.0).rgb * u.weights[k / 4][k % 4];
    }
    c = c * u.tint.rgb * pow(u.tint.a, 2.2);
    // Clouds: a plane overhead, seen thinner towards the horizon. Scales and
    // drift speeds are by eye. Sampled with the plane's gradients so distant
    // cloud uses the mipmaps instead of streaking.
    let plane = dir.xy / (max(dir.z, 0.0) + 0.12);
    let gx = dpdx(plane);
    let gy = dpdy(plane);
    let fade = smoothstep(0.06, 0.4, dir.z);
    let t = u.cloud_colour.a;
    for (var k = 0; k < 2; k++) {
        let layer = i32(u.clouds[k]);
        if layer < 0 || fade <= 0.0 {
            continue;
        }
        let scale = select(0.55, 0.8, k == 1);
        let drift = vec2<f32>(0.004, 0.0015) * select(1.0, 1.7, k == 1) * t;
        let uv = plane * scale + drift;
        let cloud = textureSampleGrad(skies, samp, uv, layer, gx * scale, gy * scale);
        let a = cloud.a * u.clouds[k + 2] * fade;
        c = mix(c, cloud.rgb * u.cloud_colour.rgb, a);
    }
    return vec4<f32>(c, 1.0);
}
