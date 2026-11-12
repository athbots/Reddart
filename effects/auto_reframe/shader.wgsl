// AI Auto Reframe Shader
// Section W3.6: Intelligent aspect ratio adaptation with dynamic viewport cropping & background blur.

struct ReframeParams {
    target_aspect: u32,    // 0: 9x16, 1: 1x1, 2: 4x5, 3: 21x9
    pan_x: f32,            // Normalized pan offset [-0.5, 0.5]
    pan_y: f32,            // Normalized pan offset [-0.5, 0.5]
    zoom: f32,             // Zoom scale [0.5, 3.0]
    fill_mode: u32,        // 0: Crop To Fill, 1: Blurred BG, 2: Black Bars
    bg_blur: f32,          // Blur radius
    bg_dim: f32,           // Dimming factor [0.0, 1.0]
    _padding: f32,
}

@group(0) @binding(0) var input_texture: texture_2d<f32>;
@group(0) @binding(1) var input_sampler: sampler;
@group(0) @binding(2) var<uniform> params: ReframeParams;

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

    // Determine target aspect ratio: 0: 9:16, 1: 1:1, 2: 4:5, 3: 21:9
    var dst_aspect: f32 = 9.0 / 16.0;
    if (params.target_aspect == 1u) {
        dst_aspect = 1.0;
    } else if (params.target_aspect == 2u) {
        dst_aspect = 4.0 / 5.0;
    } else if (params.target_aspect == 3u) {
        dst_aspect = 21.0 / 9.0;
    }

    let src_aspect: f32 = 16.0 / 9.0;
    let aspect_ratio: f32 = dst_aspect / src_aspect;

    // Viewport transform
    let center = vec2<f32>(0.5 + params.pan_x, 0.5 + params.pan_y);
    let scale = 1.0 / max(params.zoom, 0.001);

    var crop_uv = (uv - vec2<f32>(0.5, 0.5)) * scale;
    if (aspect_ratio < 1.0) {
        // Output is narrower than input (e.g. 9:16 vertical)
        crop_uv.x = crop_uv.x * aspect_ratio;
    } else {
        // Output is wider than input
        crop_uv.y = crop_uv.y / aspect_ratio;
    }
    crop_uv = crop_uv + center;

    let is_inside_crop = crop_uv.x >= 0.0 && crop_uv.x <= 1.0 && crop_uv.y >= 0.0 && crop_uv.y <= 1.0;

    if (params.fill_mode == 0u || is_inside_crop) {
        let clamped_uv = clamp(crop_uv, vec2<f32>(0.0, 0.0), vec2<f32>(1.0, 1.0));
        return textureSample(input_texture, input_sampler, clamped_uv);
    }

    if (params.fill_mode == 2u) {
        // Black bars
        return vec4<f32>(0.0, 0.0, 0.0, 1.0);
    }

    // Blurred Background Pillarbox / Letterbox (fill_mode == 1u)
    let blur_uv = uv;
    let radius = params.bg_blur * 0.002;
    var acc = vec4<f32>(0.0);

    acc += textureSample(input_texture, input_sampler, blur_uv + vec2<f32>(-radius, -radius)) * 0.075;
    acc += textureSample(input_texture, input_sampler, blur_uv + vec2<f32>(0.0, -radius)) * 0.125;
    acc += textureSample(input_texture, input_sampler, blur_uv + vec2<f32>(radius, -radius)) * 0.075;
    acc += textureSample(input_texture, input_sampler, blur_uv + vec2<f32>(-radius, 0.0)) * 0.125;
    acc += textureSample(input_texture, input_sampler, blur_uv) * 0.200;
    acc += textureSample(input_texture, input_sampler, blur_uv + vec2<f32>(radius, 0.0)) * 0.125;
    acc += textureSample(input_texture, input_sampler, blur_uv + vec2<f32>(-radius, radius)) * 0.075;
    acc += textureSample(input_texture, input_sampler, blur_uv + vec2<f32>(0.0, radius)) * 0.125;
    acc += textureSample(input_texture, input_sampler, blur_uv + vec2<f32>(radius, radius)) * 0.075;

    let dimmed = vec4<f32>(acc.rgb * params.bg_dim, acc.a);
    return dimmed;
}
