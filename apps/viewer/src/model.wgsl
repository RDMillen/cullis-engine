struct Uniforms {
    view_proj: mat4x4<f32>,
    // xyz: direction towards the sun.
    sun: vec4<f32>,
    // xyz: the camera, in the model's own space (for the highlight's rim).
    eye: vec4<f32>,
    // rgb: highlight colour; a: its strength (0: none).
    highlight: vec4<f32>,
};

@group(0) @binding(0) var<uniform> u: Uniforms;
@group(1) @binding(0) var tex: texture_2d<f32>;
@group(1) @binding(1) var samp: sampler;

struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) normal: vec3<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) local: vec3<f32>,
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
    return out;
}

// Placeholder material: the main texture, alpha-tested, with simple sun lighting.
@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let c = textureSample(tex, samp, in.uv);
    if c.a < 0.5 {
        discard;
    }
    let n = normalize(in.normal);
    let light = 0.45 + 0.55 * abs(dot(n, normalize(u.sun.xyz)));
    var rgb = c.rgb * light;
    // The interaction highlight: a glow at the silhouette's edge and a faint tint.
    if u.highlight.a > 0.0 {
        let v = normalize(u.eye.xyz - in.local);
        let rim = pow(1.0 - abs(dot(n, v)), 2.5);
        rgb = rgb + u.highlight.rgb * (rim * 0.9 + 0.03) * u.highlight.a;
    }
    return vec4<f32>(rgb, 1.0);
}
