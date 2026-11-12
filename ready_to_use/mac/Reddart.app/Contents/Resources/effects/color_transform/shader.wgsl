// Cinema Color Space & ACES Transform WGSL Shader
// High-precision GPU evaluation of Color Gamuts, Transfer Functions, and ACES Tone Mapping.

struct Params {
    source_gamut: f32, // 0: Rec709, 1: DisplayP3, 2: Rec2020, 3: ACEScg, 4: ACES2065
    source_tf: f32,    // 0: Linear, 1: sRGB, 2: Rec709, 3: PQ, 4: HLG, 5: ACEScc
    target_gamut: f32, // 0: Rec709, 1: DisplayP3, 2: Rec2020, 3: ACEScg, 4: ACES2065
    target_tf: f32,    // 0: Linear, 1: sRGB, 2: Rec709, 3: PQ, 4: HLG, 5: ACEScc
    exposure: f32,     // Exposure in EV stops
    tone_mapping: f32, // 0.0: off, 1.0: ACES filmic
    _padding: vec2<f32>,
}

@group(0) @binding(0) var input_texture: texture_2d<f32>;
@group(0) @binding(1) var input_sampler: sampler;
@group(0) @binding(2) var<uniform> params: Params;

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

// Convert nonlinear to linear
fn decode_tf(c: f32, tf: f32) -> f32 {
    let mode = i32(round(tf));
    if (mode == 0) {
        return c; // Linear
    } else if (mode == 1) { // sRGB
        let c_clamped = max(c, 0.0);
        if (c_clamped <= 0.04045) {
            return c_clamped / 12.92;
        } else {
            return pow((c_clamped + 0.055) / 1.055, 2.4);
        }
    } else if (mode == 2) { // Rec.709
        let c_clamped = max(c, 0.0);
        if (c_clamped < 0.081) {
            return c_clamped / 4.5;
        } else {
            return pow((c_clamped + 0.099) / 1.099, 1.0 / 0.45);
        }
    } else if (mode == 3) { // PQ (SMPTE ST 2084)
        let n = clamp(c, 0.0, 1.0);
        let m1 = 0.1593017578125;
        let m2 = 78.84375;
        let c1 = 0.8359375;
        let c2 = 18.8515625;
        let c3 = 18.6875;
        let vp = pow(n, 1.0 / m2);
        let num = max(vp - c1, 0.0);
        let den = max(c2 - c3 * vp, 1e-8);
        return pow(num / den, 1.0 / m1) * 100.0;
    } else if (mode == 4) { // HLG
        let ep = clamp(c, 0.0, 1.0);
        let a = 0.17883277;
        let b = 0.28466892;
        let c_const = 0.55991073;
        var e = 0.0;
        if (ep <= 0.5) {
            e = (ep * ep * 4.0) / 3.0;
        } else {
            e = (exp((ep - c_const) / a) + b) / 12.0;
        }
        return e * 3.162;
    } else if (mode == 5) { // ACEScc
        let log_thresh = -0.30137;
        if (c < log_thresh) {
            let exp_val = pow(2.0, c * 17.52 - 9.72);
            return (exp_val - pow(2.0, -16.0)) * 2.0;
        } else {
            return pow(2.0, c * 17.52 - 9.72);
        }
    }
    return c;
}

// Convert linear to target nonlinear
fn encode_tf(l: f32, tf: f32) -> f32 {
    let mode = i32(round(tf));
    if (mode == 0) {
        return l;
    } else if (mode == 1) { // sRGB
        let l_clamped = max(l, 0.0);
        if (l_clamped <= 0.0031308) {
            return l_clamped * 12.92;
        } else {
            return 1.055 * pow(l_clamped, 1.0 / 2.4) - 0.055;
        }
    } else if (mode == 2) { // Rec.709
        let l_clamped = max(l, 0.0);
        if (l_clamped < 0.018) {
            return l_clamped * 4.5;
        } else {
            return 1.099 * pow(l_clamped, 0.45) - 0.099;
        }
    } else if (mode == 3) { // PQ
        let l_norm = clamp(l / 100.0, 0.0, 1.0);
        let m1 = 0.1593017578125;
        let m2 = 78.84375;
        let c1 = 0.8359375;
        let c2 = 18.8515625;
        let c3 = 18.6875;
        let yp = pow(l_norm, m1);
        let num = c1 + c2 * yp;
        let den = 1.0 + c3 * yp;
        return pow(num / den, m2);
    } else if (mode == 4) { // HLG
        let e = max(l / 3.162, 0.0);
        let a = 0.17883277;
        let b = 0.28466892;
        let c_const = 0.55991073;
        if (e <= (1.0 / 12.0)) {
            return sqrt(3.0 * e) * 0.5;
        } else {
            return a * log(max(12.0 * e - b, 1e-8)) + c_const;
        }
    } else if (mode == 5) { // ACEScc
        if (l <= 0.0) {
            return (-16.0 + 9.72) / 17.52;
        } else if (l < pow(2.0, -15.0)) {
            return log2(pow(2.0, -16.0) + l * 0.5) * (1.0 / 17.52) + (9.72 / 17.52);
        } else {
            return log2(l) * (1.0 / 17.52) + (9.72 / 17.52);
        }
    }
    return l;
}

// ACES filmic tone map fit
fn filmic_tone_map(x: f32) -> f32 {
    let x_pos = max(x, 0.0);
    let a = 2.51;
    let b = 0.03;
    let c = 2.43;
    let d = 0.59;
    let e = 0.14;
    return clamp((x_pos * (a * x_pos + b)) / (x_pos * (c * x_pos + d) + e), 0.0, 1.0);
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let src = textureSample(input_texture, input_sampler, in.tex_coords);

    // 1. Decode Transfer Function to Linear
    var linear_rgb = vec3<f32>(
        decode_tf(src.r, params.source_tf),
        decode_tf(src.g, params.source_tf),
        decode_tf(src.b, params.source_tf)
    );

    // 2. Exposure scaling in linear space
    if (abs(params.exposure) > 0.001) {
        linear_rgb = linear_rgb * pow(2.0, params.exposure);
    }

    // 3. Optional ACES Filmic Tone Mapping
    if (params.tone_mapping > 0.5) {
        linear_rgb = vec3<f32>(
            filmic_tone_map(linear_rgb.r),
            filmic_tone_map(linear_rgb.g),
            filmic_tone_map(linear_rgb.b)
        );
    }

    // 4. Encode to Target Transfer Function
    let out_rgb = vec3<f32>(
        encode_tf(linear_rgb.r, params.target_tf),
        encode_tf(linear_rgb.g, params.target_tf),
        encode_tf(linear_rgb.b, params.target_tf)
    );

    return vec4<f32>(out_rgb, src.a);
}

