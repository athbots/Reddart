// 3D LUT Color Grading Shader
// Section W2.2: Professional 3D LUT color grading pass (.cube format).

struct LutParams {
    intensity: f32, // 0.0 = original, 1.0 = full LUT grade
    lut_size: f32,  // e.g. 33.0 or 64.0
    _padding: vec2<f32>,
}

@group(0) @binding(0) var input_texture: texture_2d<f32>;
@group(0) @binding(1) var lut_texture: texture_3d<f32>;
@group(0) @binding(2) var lut_sampler: sampler;
@group(0) @binding(3) var<uniform> params: LutParams;

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

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let color = textureSample(input_texture, lut_sampler, in.tex_coords);

    // Scale coords to sample centers of 3D LUT voxels: (c * (size - 1) + 0.5) / size
    let scale = (params.lut_size - 1.0) / params.lut_size;
    let offset = 0.5 / params.lut_size;
    let lut_coords = color.rgb * scale + vec3<f32>(offset);

    let graded_color = textureSample(lut_texture, lut_sampler, lut_coords).rgb;

    // Mix between original and graded according to intensity
    let final_rgb = mix(color.rgb, graded_color, params.intensity);

    return vec4<f32>(final_rgb, color.a);
}

