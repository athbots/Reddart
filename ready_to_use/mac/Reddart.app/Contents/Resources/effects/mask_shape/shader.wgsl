// Mask Shape Shader
// Evaluates geometric shape masks (rectangle, ellipse, gradient, vignette)
// with edge feathering and invert controls.

struct MaskParams {
    shape_type: u32,  // 0=Rectangle, 1=Ellipse, 2=Linear Gradient, 3=Vignette
    center_x: f32,    // 0.0 to 1.0 (default 0.5)
    center_y: f32,    // 0.0 to 1.0 (default 0.5)
    size_x: f32,      // 0.0 to 1.0 (default 0.4)
    size_y: f32,      // 0.0 to 1.0 (default 0.4)
    feather: f32,     // 0.001 to 0.5 (default 0.05)
    invert: u32,      // 0=Normal, 1=Invert
    _padding: f32,
}

@group(0) @binding(0) var input_texture: texture_2d<f32>;
@group(0) @binding(1) var tex_sampler: sampler;
@group(0) @binding(2) var<uniform> params: MaskParams;

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
    let color = textureSample(input_texture, tex_sampler, in.tex_coords);
    let uv = in.tex_coords;
    let center = vec2<f32>(params.center_x, params.center_y);
    let delta = uv - center;

    var mask_alpha: f32 = 1.0;

    if (params.shape_type == 0u) {
        // Rectangle mask SDF
        let d = abs(delta) - vec2<f32>(params.size_x, params.size_y);
        let dist = max(d.x, d.y);
        mask_alpha = 1.0 - smoothstep(-params.feather, 0.0, dist);
    } else if (params.shape_type == 1u) {
        // Ellipse mask SDF
        let norm_d = delta / max(vec2<f32>(params.size_x, params.size_y), vec2<f32>(0.001, 0.001));
        let dist = length(norm_d) - 1.0;
        mask_alpha = 1.0 - smoothstep(-params.feather, 0.0, dist);
    } else if (params.shape_type == 2u) {
        // Linear gradient (wipe mask along Y axis)
        let t = (uv.y - (center.y - params.size_y)) / max(params.size_y * 2.0, 0.001);
        mask_alpha = smoothstep(0.0, 1.0, clamp(t, 0.0, 1.0));
    } else {
        // Vignette: gentle radial falloff from center
        let dist = length(delta);
        let radius = params.size_x;
        mask_alpha = 1.0 - smoothstep(radius - params.feather, radius + params.feather, dist);
    }

    if (params.invert == 1u) {
        mask_alpha = 1.0 - mask_alpha;
    }

    return vec4<f32>(color.rgb, color.a * clamp(mask_alpha, 0.0, 1.0));
}

