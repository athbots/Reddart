// Brightness / Contrast effect shader
// This is the first WGSL effect for the Reddart pipeline.
// It demonstrates the effect spec pattern: a single WGSL file
// that runs identically on all platforms via wgpu.

// Uniforms for the effect parameters
struct Params {
    brightness: f32,  // -1.0 to 1.0 (0.0 = no change)
    contrast: f32,    // -1.0 to 1.0 (0.0 = no change)
    _padding: vec2<f32>,
}

@group(0) @binding(0) var input_texture: texture_2d<f32>;
@group(0) @binding(1) var input_sampler: sampler;
@group(0) @binding(2) var<uniform> params: Params;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) tex_coords: vec2<f32>,
}

// Full-screen triangle vertex shader
@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> VertexOutput {
    var out: VertexOutput;
    // Generate a full-screen triangle (3 vertices, no vertex buffer needed)
    let x = f32(i32(vertex_index & 1u) * 4 - 1);
    let y = f32(i32(vertex_index & 2u) * 2 - 1);
    out.position = vec4<f32>(x, y, 0.0, 1.0);
    out.tex_coords = vec2<f32>((x + 1.0) * 0.5, (1.0 - y) * 0.5);
    return out;
}

// Fragment shader: apply brightness and contrast
@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let color = textureSample(input_texture, input_sampler, in.tex_coords);

    // Apply brightness (additive)
    var rgb = color.rgb + vec3<f32>(params.brightness);

    // Apply contrast (around midpoint 0.5)
    let contrast_factor = 1.0 + params.contrast;
    rgb = (rgb - 0.5) * contrast_factor + 0.5;

    // Clamp to valid range
    rgb = clamp(rgb, vec3<f32>(0.0), vec3<f32>(1.0));

    return vec4<f32>(rgb, color.a);
}
