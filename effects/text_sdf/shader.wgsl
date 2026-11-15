// Signed Distance Field (SDF) Text Rendering Shader
// Section W1.5: Sharp, resolution-independent text overlays with outlines and drop shadows.

struct TextUniforms {
    text_color: vec4<f32>,
    outline_color: vec4<f32>,
    shadow_color: vec4<f32>,
    outline_width: f32,       // 0.0 = no outline, up to 0.5 in distance space
    smoothing: f32,           // Edge antialiasing smoothing width (typically 0.02..0.05)
    shadow_smoothing: f32,    // Shadow edge softness (typically 0.05..0.15)
    enable_shadow: f32,       // 1.0 = enabled, 0.0 = disabled
    shadow_offset: vec2<f32>, // Offset in UV texture space
    _padding: vec2<f32>,      // 8-byte alignment padding for 16-byte struct size
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

    // 1. Text core interior alpha
    let text_alpha = smoothstep(0.5 - params.smoothing, 0.5 + params.smoothing, distance) * params.text_color.a;

    // 2. Stroke outline alpha
    var outline_alpha = 0.0;
    if (params.outline_width > 0.0) {
        let outline_threshold = 0.5 - params.outline_width;
        let total_coverage = smoothstep(outline_threshold - params.smoothing, outline_threshold + params.smoothing, distance);
        let inner_norm = text_alpha / max(params.text_color.a, 0.0001);
        outline_alpha = max(0.0, total_coverage - inner_norm) * params.outline_color.a;
    }

    // 3. Drop shadow alpha (sampled at offset UV)
    var shadow_alpha = 0.0;
    if (params.enable_shadow > 0.5) {
        let shadow_uv = in.tex_coords - params.shadow_offset;
        let shadow_dist = textureSample(font_atlas, font_sampler, shadow_uv).r;
        let s_thresh = 0.5 - params.outline_width;
        shadow_alpha = smoothstep(s_thresh - params.shadow_smoothing, s_thresh + params.shadow_smoothing, shadow_dist) * params.shadow_color.a;
    }

    // Composite layers back-to-front: Shadow -> Outline -> Interior
    // Layer 1: Shadow base
    var acc_color = params.shadow_color.rgb;
    var acc_alpha = shadow_alpha;

    // Layer 2: Outline over Shadow
    let out_a = outline_alpha + acc_alpha * (1.0 - outline_alpha);
    if (out_a > 0.0001) {
        acc_color = (params.outline_color.rgb * outline_alpha + acc_color * acc_alpha * (1.0 - outline_alpha)) / out_a;
        acc_alpha = out_a;
    }

    // Layer 3: Text Interior over Outline & Shadow
    let final_a = text_alpha + acc_alpha * (1.0 - text_alpha);
    var final_rgb = acc_color;
    if (final_a > 0.0001) {
        final_rgb = (params.text_color.rgb * text_alpha + acc_color * acc_alpha * (1.0 - text_alpha)) / final_a;
    }

    return vec4<f32>(final_rgb, final_a);
}
