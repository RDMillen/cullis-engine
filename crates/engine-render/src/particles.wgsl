// Particles: camera-facing sprites from a texture array, tinted by their colour
// over life. Flames add (their sprites are drawn on black); smoke blends by
// alpha.

struct Uniforms {
    view_proj: mat4x4<f32>,
};

@group(0) @binding(0) var<uniform> u: Uniforms;
@group(0) @binding(1) var sprites: texture_2d_array<f32>;
@group(0) @binding(2) var samp: sampler;

struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) colour: vec4<f32>,
    @location(2) @interpolate(flat) layer: i32,
};

@vertex
fn vs_main(
    @location(0) position: vec3<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) colour: vec4<f32>,
    @location(3) layer: f32,
) -> VsOut {
    var out: VsOut;
    out.clip = u.view_proj * vec4<f32>(position, 1.0);
    out.uv = uv;
    out.colour = colour;
    out.layer = i32(layer);
    return out;
}

@fragment
fn fs_additive(in: VsOut) -> @location(0) vec4<f32> {
    let t = textureSample(sprites, samp, in.uv, in.layer);
    return vec4<f32>(t.rgb * in.colour.rgb * in.colour.a, 1.0);
}

@fragment
fn fs_alpha(in: VsOut) -> @location(0) vec4<f32> {
    let t = textureSample(sprites, samp, in.uv, in.layer);
    return vec4<f32>(t.rgb * in.colour.rgb, t.a * in.colour.a);
}
