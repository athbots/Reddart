// Composite blend shader
// Blends two layers together with configurable opacity and blend mode.

struct BlendParams {
    opacity: f32,     // 0.0 to 1.0
    blend_mode: u32,  // 0=Normal, 1=Multiply, 2=Screen, 3=Overlay
    _padding: vec2<f32>,
}

@group(0) @binding(0) var base_texture: texture_2d<f32>;
@group(0) @binding(1) var overlay_texture: texture_2d<f32>;
@group(0) @binding(2) var tex_sampler: sampler;
@group(0) @binding(3) var<uniform> params: BlendParams;

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

// Blend mode functions
fn blend_multiply(base: vec3<f32>, overlay: vec3<f32>) -> vec3<f32> {
    return base * overlay;
}

fn blend_screen(base: vec3<f32>, overlay: vec3<f32>) -> vec3<f32> {
    return 1.0 - (1.0 - base) * (1.0 - overlay);
}

fn blend_overlay(base: vec3<f32>, overlay: vec3<f32>) -> vec3<f32> {
    let r = select(
        1.0 - 2.0 * (1.0 - base.r) * (1.0 - overlay.r),
        2.0 * base.r * overlay.r,
        base.r < 0.5
    );
    let g = select(
        1.0 - 2.0 * (1.0 - base.g) * (1.0 - overlay.g),
        2.0 * base.g * overlay.g,
        base.g < 0.5
    );
    let b = select(
        1.0 - 2.0 * (1.0 - base.b) * (1.0 - overlay.b),
        2.0 * base.b * overlay.b,
        base.b < 0.5
    );
    return vec3<f32>(r, g, b);
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let base = textureSample(base_texture, tex_sampler, in.tex_coords);
    let overlay = textureSample(overlay_texture, tex_sampler, in.tex_coords);

    var blended: vec3<f32>;
    switch params.blend_mode {
        case 1u: { blended = blend_multiply(base.rgb, overlay.rgb); }
        case 2u: { blended = blend_screen(base.rgb, overlay.rgb); }
        case 3u: { blended = blend_overlay(base.rgb, overlay.rgb); }
        default: { blended = overlay.rgb; }  // Normal: just use overlay
    }

    // Mix with opacity
    let result = mix(base.rgb, blended, params.opacity * overlay.a);
    return vec4<f32>(result, base.a);
}
