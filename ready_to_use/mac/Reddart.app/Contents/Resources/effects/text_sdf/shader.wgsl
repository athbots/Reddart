// Signed Distance Field (SDF) Text Rendering Shader
// Section W1.5: Sharp, resolution-independent text overlays with outlines and shadows.

struct TextUniforms {
    text_color: vec4<f32>,
    outline_color: vec4<f32>,
    outline_width: f32, // 0.0 = no outline, up to 0.5
    smoothing: f32,     // Edge smoothing (antialiasing width)
    _padding: vec2<f32>,
}

@group(0) @binding(0) var font_atlas: texture_2d<f32>;
@group(0) @binding(1) var font_sampler: sampler;
@group(0) @binding(2) var<uniform> params: TextUniforms;

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
    let distance = textureSample(font_atlas, font_sampler, in.tex_coords).r;

    // Interior alpha
    let alpha = smoothstep(0.5 - params.smoothing, 0.5 + params.smoothing, distance);

    // Outline alpha
    var final_color = params.text_color;
    var final_alpha = alpha * params.text_color.a;

    if (params.outline_width > 0.0) {
        let outline_threshold = 0.5 - params.outline_width;
        let outline_alpha = smoothstep(outline_threshold - params.smoothing, outline_threshold + params.smoothing, distance);
        final_color = mix(params.outline_color, params.text_color, alpha);
        final_alpha = outline_alpha * max(params.text_color.a, params.outline_color.a);
    }

    return vec4<f32>(final_color.rgb, final_alpha);
}

