// Chroma Key Shader with Green/Blue Spill Suppression
// Section W2.3: Studio-quality on-device chroma keying in YUV color space.

struct ChromaParams {
    key_color: vec3<f32>,   // Target screen color (usually green [0.0, 1.0, 0.0])
    similarity: f32,        // Color distance threshold (0.0 to 1.0)
    smoothness: f32,        // Edge alpha blend width (0.0 to 0.5)
    spill: f32,             // Spill suppression intensity (0.0 to 1.0)
}

@group(0) @binding(0) var input_texture: texture_2d<f32>;
@group(0) @binding(1) var input_sampler: sampler;
@group(0) @binding(2) var<uniform> params: ChromaParams;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) tex_coords: vec2<f32>,
}

@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> VertexOutput {
    var out: VertexOutput;
    let x = f32(i32(vertex_index & 1u) * 4 - 1);
    let y = f32(i32(vertex_index & 2u) * 2 - 1);
    out.position = vec4<f32>(x, y, 0.0, 1.0);
    out.tex_coords = vec2<f32>((x + 1.0) * 0.5, (1.0 - y) * 0.5);
    return out;
}

fn rgb_to_yuv(rgb: vec3<f32>) -> vec3<f32> {
    let y = 0.299 * rgb.r + 0.587 * rgb.g + 0.114 * rgb.b;
    let u = -0.14713 * rgb.r - 0.28886 * rgb.g + 0.436 * rgb.b;
    let v = 0.615 * rgb.r - 0.51499 * rgb.g - 0.10001 * rgb.b;
    return vec3<f32>(y, u, v);
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let color = textureSample(input_texture, input_sampler, in.tex_coords);

    let pixel_yuv = rgb_to_yuv(color.rgb);
    let key_yuv = rgb_to_yuv(params.key_color);

    // Compute chrominance distance in UV space
    let dist = distance(pixel_yuv.yz, key_yuv.yz);

    // Edge-smoothed keying
    let base_mask = dist - params.similarity;
    let alpha = smoothstep(0.0, params.smoothness, base_mask);

    // Despill: suppress green tint reflection on hair and edges
    var despilled_rgb = color.rgb;
    let green_spill = despilled_rgb.g - max(despilled_rgb.r, despilled_rgb.b);
    if (green_spill > 0.0) {
        despilled_rgb.g = despilled_rgb.g - green_spill * params.spill;
    }

    return vec4<f32>(despilled_rgb, color.a * alpha);
}

