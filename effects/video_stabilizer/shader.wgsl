// AI Video Stabilizer Shader
// Section W3.6: Kinematic camera motion compensation, dynamic margin auto-crop, and border synthesis.

struct StabilizerParams {
    comp_dx: f32,       // Compensatory X translation in normalized coordinate space [-1.0, 1.0]
    comp_dy: f32,       // Compensatory Y translation in normalized coordinate space [-1.0, 1.0]
    comp_dtheta: f32,   // Compensatory rotation in radians
    comp_scale: f32,    // Compensatory zoom scale multiplier (typically ~1.0)
    zoom: f32,          // Dynamic margin auto-crop zoom factor (>= 1.0)
    border_mode: u32,   // 0: AutoCrop, 1: Mirror, 2: Replicate, 3: BlurredMargin, 4: Transparent
    blur_radius: f32,   // Blur radius for BlurredMargin mode
    _padding: f32,      // 16-byte struct alignment padding (total 32 bytes)
}

@group(0) @binding(0) var input_texture: texture_2d<f32>;
@group(0) @binding(1) var input_sampler: sampler;
@group(0) @binding(2) var<uniform> params: StabilizerParams;

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
    let uv = in.tex_coords;
    let center = vec2<f32>(0.5, 0.5);

    // 1. Center coordinates
    let p = uv - center;

    // 2. Dynamic margin zoom and compensatory scale
    let eff_zoom = max(params.zoom * params.comp_scale, 0.001);
    let p_scaled = p / eff_zoom;

    // 3. Compensatory rotation (apply inverse rotation to undo camera jitter)
    let cos_t = cos(params.comp_dtheta);
    let sin_t = sin(params.comp_dtheta);
    let p_rot = vec2<f32>(
        p_scaled.x * cos_t + p_scaled.y * sin_t,
        -p_scaled.x * sin_t + p_scaled.y * cos_t
    );

    // 4. Compensatory translation (subtract displacement to stabilize)
    var src_uv = p_rot - vec2<f32>(params.comp_dx, params.comp_dy) + center;

    // Check if transformed coordinate lies within the valid source image canvas
    let is_inside = src_uv.x >= 0.0 && src_uv.x <= 1.0 && src_uv.y >= 0.0 && src_uv.y <= 1.0;

    // Mode 0: AutoCrop (or inside bounds)
    if (params.border_mode == 0u || is_inside) {
        let clamped = clamp(src_uv, vec2<f32>(0.0, 0.0), vec2<f32>(1.0, 1.0));
        return textureSample(input_texture, input_sampler, clamped);
    }

    // Mode 1: Mirror borders
    if (params.border_mode == 1u) {
        var mir_u = src_uv.x;
        var mir_v = src_uv.y;
        if (mir_u < 0.0) { mir_u = -mir_u; }
        else if (mir_u > 1.0) { mir_u = 2.0 - mir_u; }
        if (mir_v < 0.0) { mir_v = -mir_v; }
        else if (mir_v > 1.0) { mir_v = 2.0 - mir_v; }
        let clamped_mir = clamp(vec2<f32>(mir_u, mir_v), vec2<f32>(0.0, 0.0), vec2<f32>(1.0, 1.0));
        return textureSample(input_texture, input_sampler, clamped_mir);
    }

    // Mode 2: Replicate edge clamp
    if (params.border_mode == 2u) {
        let clamped = clamp(src_uv, vec2<f32>(0.0, 0.0), vec2<f32>(1.0, 1.0));
        return textureSample(input_texture, input_sampler, clamped);
    }

    // Mode 3: Blurred peripheral margin
    if (params.border_mode == 3u) {
        let blur_r = max(params.blur_radius, 1.0) * 0.002;
        let clamped_base = clamp(src_uv, vec2<f32>(0.0, 0.0), vec2<f32>(1.0, 1.0));
        var acc = vec4<f32>(0.0);

        acc += textureSample(input_texture, input_sampler, clamped_base + vec2<f32>(-blur_r, -blur_r)) * 0.075;
        acc += textureSample(input_texture, input_sampler, clamped_base + vec2<f32>(0.0, -blur_r)) * 0.125;
        acc += textureSample(input_texture, input_sampler, clamped_base + vec2<f32>(blur_r, -blur_r)) * 0.075;
        acc += textureSample(input_texture, input_sampler, clamped_base + vec2<f32>(-blur_r, 0.0)) * 0.125;
        acc += textureSample(input_texture, input_sampler, clamped_base) * 0.200;
        acc += textureSample(input_texture, input_sampler, clamped_base + vec2<f32>(blur_r, 0.0)) * 0.125;
        acc += textureSample(input_texture, input_sampler, clamped_base + vec2<f32>(-blur_r, blur_r)) * 0.075;
        acc += textureSample(input_texture, input_sampler, clamped_base + vec2<f32>(0.0, blur_r)) * 0.125;
        acc += textureSample(input_texture, input_sampler, clamped_base + vec2<f32>(blur_r, blur_r)) * 0.075;

        // Subtle peripheral vignette dimming
        return vec4<f32>(acc.rgb * 0.7, acc.a);
    }

    // Mode 4: Transparent / Black canvas
    return vec4<f32>(0.0, 0.0, 0.0, 0.0);
}
