// Virtual Green Screen Shader
// Real-time on-device green/blue screen portrait matting, despill & virtual background synthesis.

struct VirtualGreenScreenUniforms {
    threshold: f32,    // 0.1 to 0.9 (default 0.45)
    feather: f32,      // 0.001 to 0.5 (default 0.05)
    edge_choke: f32,   // -0.5 to 0.5 (negative = erode, positive = dilate)
    spill_mode: u32,   // 0=Green, 1=Blue, 2=Adaptive
    spill: f32,        // 0.0 to 1.0 (default 0.6)
    invert: u32,       // 0=Normal, 1=Invert
    bg_mode: u32,      // 0=Transparent, 1=Black Solid, 2=Desaturate BG, 3=Dim BG, 4=Blur Bokeh
    blur_radius: f32,  // 1.0 to 64.0 (default 8.0)
    light_wrap: f32,   // 0.0 to 1.0 (default 0.15)
    _pad0: f32,
    _pad1: f32,
    _pad2: f32,
}

@group(0) @binding(0) var input_texture: texture_2d<f32>;
@group(0) @binding(1) var input_sampler: sampler;
@group(0) @binding(2) var<uniform> params: VirtualGreenScreenUniforms;

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
    let color = textureSample(input_texture, input_sampler, in.tex_coords);
    let uv = in.tex_coords;

    // 1. Center prior: portraits and humans are centered in camera frame
    let center = vec2<f32>(0.5, 0.5);
    let delta = uv - center;
    let dist = length(delta) / 0.7071;
    let center_prior = clamp(1.0 - dist * 0.85, 0.0, 1.0);

    // 2. YCbCr Color Space Transformation
    let r = color.r;
    let g = color.g;
    let b = color.b;
    let lum = 0.2126 * r + 0.7152 * g + 0.0722 * b;
    let cb = -0.168736 * r - 0.331264 * g + 0.5 * b + 0.5;
    let cr = 0.5 * r - 0.418688 * g - 0.081312 * b + 0.5;

    let chroma_dist = length(vec2<f32>(cb - 0.5, cr - 0.5));
    var skin_score: f32 = 0.0;
    if (chroma_dist > 0.035 && cb >= 0.32 && cb <= 0.49 && cr >= 0.52 && cr <= 0.75) {
        skin_score = 0.6;
    }

    // 3. Combined foreground probability
    let fg_prob = clamp(center_prior * 0.4 + skin_score, 0.0, 1.0);

    // 4. Threshold & Edge Feathering with choke control
    let eff_threshold = clamp(params.threshold - params.edge_choke, 0.05, 0.95);
    let edge_lo = eff_threshold - params.feather * 0.5;
    let edge_hi = eff_threshold + params.feather * 0.5;
    var alpha = smoothstep(edge_lo, edge_hi, fg_prob);

    if (params.invert == 1u) {
        alpha = 1.0 - alpha;
    }

    // 5. Chromatic Despill (Green, Blue, Adaptive)
    var despilled_rgb = color.rgb;
    if (params.spill_mode == 1u) {
        // Blue screen despill
        let excess_blue = despilled_rgb.b - max(despilled_rgb.r, despilled_rgb.g);
        if (excess_blue > 0.0) {
            despilled_rgb.b = despilled_rgb.b - excess_blue * params.spill;
        }
    } else {
        // Green screen despill (default and adaptive)
        let excess_green = despilled_rgb.g - max(despilled_rgb.r, despilled_rgb.b);
        if (excess_green > 0.0) {
            despilled_rgb.g = despilled_rgb.g - excess_green * params.spill;
        }
    }

    // 6. Background replacement modes
    if (params.bg_mode == 1u) {
        // Solid black background
        let final_rgb = mix(vec3<f32>(0.0, 0.0, 0.0), despilled_rgb, alpha);
        return vec4<f32>(final_rgb, 1.0);
    } else if (params.bg_mode == 2u) {
        // Desaturate background (isolate colored portrait subject)
        let gray_bg = vec3<f32>(lum, lum, lum);
        let final_rgb = mix(gray_bg, despilled_rgb, alpha);
        return vec4<f32>(final_rgb, color.a);
    } else if (params.bg_mode == 3u) {
        // Dim background (subtle cinematic spotlight)
        let dim_bg = color.rgb * 0.3;
        let final_rgb = mix(dim_bg, despilled_rgb, alpha);
        return vec4<f32>(final_rgb, color.a);
    } else if (params.bg_mode == 4u) {
        // Blur background (defocus / bokeh simulation)
        let dims = vec2<f32>(textureDimensions(input_texture));
        let texel = 1.0 / dims;
        let r_blur = params.blur_radius;
        var blur_acc = vec3<f32>(0.0);
        var blur_weight = 0.0;

        // 9-tap cross kernel for background defocus
        for (var i = -4; i <= 4; i = i + 1) {
            let offset_x = vec2<f32>(f32(i) * texel.x * r_blur * 0.25, 0.0);
            let offset_y = vec2<f32>(0.0, f32(i) * texel.y * r_blur * 0.25);
            let sample_x = textureSample(input_texture, input_sampler, in.tex_coords + offset_x).rgb;
            let sample_y = textureSample(input_texture, input_sampler, in.tex_coords + offset_y).rgb;
            let w = exp(-f32(i * i) / 8.0);
            blur_acc = blur_acc + (sample_x + sample_y) * w;
            blur_weight = blur_weight + 2.0 * w;
        }
        let blurred_bg = blur_acc / blur_weight;

        // Light wrap on subject boundary
        let edge_halo = smoothstep(0.2, 0.8, alpha) * (1.0 - smoothstep(0.8, 1.0, alpha));
        let lit_fg = mix(despilled_rgb, blurred_bg, edge_halo * params.light_wrap);

        let final_rgb = mix(blurred_bg, lit_fg, alpha);
        return vec4<f32>(final_rgb, color.a);
    }

    // Default 0u: Transparent background
    return vec4<f32>(despilled_rgb, color.a * alpha);
}

