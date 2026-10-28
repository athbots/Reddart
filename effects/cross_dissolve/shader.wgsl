// Cross-Dissolve Transition Shader
// Blends between outgoing (from) and incoming (to) textures based on progress (0.0 to 1.0)

struct TransitionParams {
    progress: f32, // 0.0 = 100% from_texture, 1.0 = 100% to_texture
    _padding: vec3<f32>,
}

@group(0) @binding(0) var from_texture: texture_2d<f32>;
@group(0) @binding(1) var to_texture: texture_2d<f32>;
@group(0) @binding(2) var tex_sampler: sampler;
@group(0) @binding(3) var<uniform> params: TransitionParams;

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
    let from_color = textureSample(from_texture, tex_sampler, in.tex_coords);
    let to_color = textureSample(to_texture, tex_sampler, in.tex_coords);
    return mix(from_color, to_color, params.progress);
}

