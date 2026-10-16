struct Uniforms {
    view_proj: mat4x4<f32>,
    // xyz: direction towards the sun.
    sun: vec4<f32>,
    // Theme grid origin (xy) and size (zw) in world units.
    grid: vec4<f32>,
    // x: world units per texture repeat.
    params: vec4<f32>,
    // rgb: the sun's and the ambient light's colours (gamma space).
    sun_colour: vec4<f32>,
    ambient: vec4<f32>,
};

@group(0) @binding(0) var<uniform> u: Uniforms;
// Per grid point: first layer, second layer, first weight, second weight.
@group(0) @binding(1) var theme_grid: texture_2d<u32>;
@group(0) @binding(2) var base_tex: texture_2d_array<f32>;
@group(0) @binding(3) var cliff_tex: texture_2d_array<f32>;
@group(0) @binding(4) var samp: sampler;

struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) normal: vec3<f32>,
    @location(1) world: vec3<f32>,
};

@vertex
fn vs_main(@location(0) position: vec3<f32>, @location(1) normal: vec3<f32>) -> VsOut {
    var out: VsOut;
    out.clip = u.view_proj * vec4<f32>(position, 1.0);
    out.normal = normal;
    out.world = position;
    return out;
}

struct Blend {
    base: vec3<f32>,
    cliff: vec3<f32>,
};

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
    return out;
}

// Light from the sun and the ambient light, as in model.wgsl.
fn lit(n: vec3<f32>) -> vec3<f32> {
    let ndl = max(dot(n, normalize(u.sun.xyz)), 0.0);
    let sum = clamp(u.ambient.rgb + u.sun_colour.rgb * ndl, vec3<f32>(0.0), vec3<f32>(1.0));
    return pow(sum, vec3<f32>(2.2));
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
    if abs(n.y) > abs(n.x) {
        cliff_uv = in.world.xz / u.params.x;
    }
    var b = Blend(vec3<f32>(0.0), vec3<f32>(0.0));
    b = add_point(b, p, (1.0 - f.x) * (1.0 - f.y), uv, cliff_uv);
    b = add_point(b, p + vec2<i32>(1, 0), f.x * (1.0 - f.y), uv, cliff_uv);
    b = add_point(b, p + vec2<i32>(0, 1), (1.0 - f.x) * f.y, uv, cliff_uv);
    b = add_point(b, p + vec2<i32>(1, 1), f.x * f.y, uv, cliff_uv);
    let albedo = mix(b.cliff, b.base, smoothstep(0.55, 0.8, n.z));
    return vec4<f32>(albedo * lit(n), 1.0);
}

// Flat placeholder sea: translucent, slightly lit so it reads as a surface.
@fragment
fn fs_water(in: VsOut) -> @location(0) vec4<f32> {
    let light = lit(normalize(in.normal)) * 0.6 + pow(u.ambient.rgb, vec3<f32>(2.2)) * 0.4;
    return vec4<f32>(vec3<f32>(0.10, 0.32, 0.45) * light * 1.6, 0.8);
}

// Navigation overlay: flat colour carried in the normal attribute, translucent.
@fragment
fn fs_overlay(in: VsOut) -> @location(0) vec4<f32> {
    return vec4<f32>(in.normal, 0.45);
}
