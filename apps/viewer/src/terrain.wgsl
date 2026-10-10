struct Uniforms {
    view_proj: mat4x4<f32>,
    // xyz: direction towards the sun.
    sun: vec4<f32>,
};

@group(0) @binding(0) var<uniform> u: Uniforms;

struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) normal: vec3<f32>,
    @location(1) height: f32,
};

@vertex
fn vs_main(@location(0) position: vec3<f32>, @location(1) normal: vec3<f32>) -> VsOut {
    var out: VsOut;
    out.clip = u.view_proj * vec4<f32>(position, 1.0);
    out.normal = normal;
    out.height = position.z;
    return out;
}

// Placeholder colouring until texture themes are decoded: sand near sea level,
// grass above it, rock on steep slopes.
@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let n = normalize(in.normal);
    let sand = vec3<f32>(0.76, 0.70, 0.50);
    let grass = vec3<f32>(0.33, 0.50, 0.22);
    let rock = vec3<f32>(0.45, 0.42, 0.40);
    var albedo = mix(sand, grass, smoothstep(2.5, 5.0, in.height));
    albedo = mix(albedo, rock, smoothstep(0.75, 0.55, n.z));
    let light = 0.35 + 0.65 * max(dot(n, normalize(u.sun.xyz)), 0.0);
    return vec4<f32>(albedo * light, 1.0);
}

// Flat placeholder sea: translucent, slightly lit so it reads as a surface.
@fragment
fn fs_water(in: VsOut) -> @location(0) vec4<f32> {
    let light = 0.6 + 0.4 * max(dot(normalize(in.normal), normalize(u.sun.xyz)), 0.0);
    return vec4<f32>(vec3<f32>(0.10, 0.32, 0.45) * light, 0.8);
}
