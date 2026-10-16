// The sky: the environment's sky textures, blended by weight and tinted, on a
// full-screen triangle behind everything. Each texture is a panorama, the
// zenith at its top and the horizon at its bottom, centred on the sun.

struct Uniforms {
    inv_view_proj: mat4x4<f32>,
    // xyz: the camera.
    eye: vec4<f32>,
    // rgb: tint (linear).
    tint: vec4<f32>,
    // Weight of each array layer.
    weights: vec4<f32>,
    // xyz: towards the sun.
    sun: vec4<f32>,
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
    let v = clamp(1.0 - elevation / 1.5707964, 0.0, 0.97);
    let azimuth = atan2(dir.y, dir.x) - atan2(u.sun.y, u.sun.x);
    let uv = vec2<f32>(0.5 + azimuth / 6.2831853, v);
    var c = vec3<f32>(0.0);
    for (var k = 0; k < 4; k++) {
        c += textureSampleLevel(skies, samp, uv, k, 0.0).rgb * u.weights[k];
    }
    return vec4<f32>(c * u.tint.rgb, 1.0);
}
