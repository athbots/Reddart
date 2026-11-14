// Horn-Schunck Iterative Optical Flow Estimation — GPU Compute Shader
//
// Estimates a dense bidirectional motion vector field between two luminance frames
// using the variational Horn-Schunck energy minimization method.
//
// Dispatch: one thread per output texel.
// Workgroup: 8x8 threads (adjustable; wgpu requires multiple of 1 on WebGPU).
//
// Inputs:
//   luma_a    — R32Float luminance texture for Frame A
//   luma_b    — R32Float luminance texture for Frame B
//   flow_in   — Rgba32Float texture carrying the previous iteration's flow estimate
//               (RG = forward u,v; BA = backward u,v), initialized to 0.5 for the
//               first iteration so that the decode formula maps to zero displacement.
//
// Output:
//   flow_out  — Rgba32Float storage texture; same layout as flow_in.
//               Encoded as: R = (u_fwd / max_disp + 1.0) * 0.5
//                           G = (v_fwd / max_disp + 1.0) * 0.5
//                           B = (u_bwd / max_disp + 1.0) * 0.5
//               A = (v_bwd / max_disp + 1.0) * 0.5
//               The warp fragment shader uses the identical [0,1] encoding.
//
// Parameters (uniform):
//   alpha          — Horn-Schunck regularization weight (larger = smoother flow).
//   max_displacement — Maximum displacement magnitude in pixels used for
//                      normalization. Vectors larger than this are clamped.
//   iteration      — Current iteration index (unused at runtime; informational).
//   _pad           — Padding to satisfy 16-byte uniform alignment.

struct HsParams {
    alpha: f32,
    max_displacement: f32,
    iteration: u32,
    _pad: u32,
}

@group(0) @binding(0) var luma_a: texture_2d<f32>;
@group(0) @binding(1) var luma_b: texture_2d<f32>;
@group(0) @binding(2) var flow_in: texture_2d<f32>;
@group(0) @binding(3) var flow_out: texture_storage_2d<rgba16float, write>;
@group(0) @binding(4) var<uniform> params: HsParams;

// ----------------------------------------------------------------------------
// Helpers
// ----------------------------------------------------------------------------

/// Load a single-channel luminance value, clamping coordinates to [0, dim-1].
fn load_luma(tex: texture_2d<f32>, coord: vec2<i32>) -> f32 {
    let dims = vec2<i32>(textureDimensions(tex));
    let c = clamp(coord, vec2<i32>(0), dims - vec2<i32>(1));
    return textureLoad(tex, c, 0).r;
}

/// Load a full RGBA flow sample from flow_in, clamping coordinates.
fn load_flow(coord: vec2<i32>) -> vec4<f32> {
    let dims = vec2<i32>(textureDimensions(flow_in));
    let c = clamp(coord, vec2<i32>(0), dims - vec2<i32>(1));
    return textureLoad(flow_in, c, 0);
}

/// Decode a single normalized channel back to signed displacement in pixels.
fn decode(v: f32, max_d: f32) -> f32 {
    return (v * 2.0 - 1.0) * max_d;
}

/// Encode a signed displacement in pixels to a normalized [0,1] channel.
fn encode(d: f32, max_d: f32) -> f32 {
    return clamp((d / max_d + 1.0) * 0.5, 0.0, 1.0);
}

// ----------------------------------------------------------------------------
// Compute entry point
// ----------------------------------------------------------------------------

@compute @workgroup_size(8, 8, 1)
fn cs_main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let dims = vec2<i32>(textureDimensions(flow_out));
    let px = vec2<i32>(gid.xy);

    if (px.x >= dims.x || px.y >= dims.y) {
        return;
    }

    let alpha = params.alpha;
    let max_d = params.max_displacement;

    // --- Sobel spatial gradients of Frame A luminance ---
    // Using a 3x3 Sobel kernel:
    //   Ix = ( -1·A(-1,-1) + 1·A(+1,-1) + -2·A(-1,0) + 2·A(+1,0) + -1·A(-1,+1) + 1·A(+1,+1) ) / 8
    //   Iy = ( -1·A(-1,-1) - 2·A(0,-1) - 1·A(+1,-1) + 1·A(-1,+1) + 2·A(0,+1) + 1·A(+1,+1) ) / 8
    let a_mm = load_luma(luma_a, px + vec2<i32>(-1, -1));
    let a_0m = load_luma(luma_a, px + vec2<i32>( 0, -1));
    let a_pm = load_luma(luma_a, px + vec2<i32>( 1, -1));
    let a_m0 = load_luma(luma_a, px + vec2<i32>(-1,  0));
    let a_p0 = load_luma(luma_a, px + vec2<i32>( 1,  0));
    let a_mp = load_luma(luma_a, px + vec2<i32>(-1,  1));
    let a_0p = load_luma(luma_a, px + vec2<i32>( 0,  1));
    let a_pp = load_luma(luma_a, px + vec2<i32>( 1,  1));

    let Ix = (-a_mm + a_pm - 2.0*a_m0 + 2.0*a_p0 - a_mp + a_pp) / 8.0;
    let Iy = (-a_mm - 2.0*a_0m - a_pm + a_mp + 2.0*a_0p + a_pp) / 8.0;

    // --- Temporal derivative between Frame A and Frame B ---
    let lum_a = load_luma(luma_a, px);
    let lum_b = load_luma(luma_b, px);
    let It = lum_b - lum_a;

    // --- Laplacian average of the FORWARD flow from neighbors (u_fwd, v_fwd) ---
    let f_m0 = load_flow(px + vec2<i32>(-1,  0));
    let f_p0 = load_flow(px + vec2<i32>( 1,  0));
    let f_0m = load_flow(px + vec2<i32>( 0, -1));
    let f_0p = load_flow(px + vec2<i32>( 0,  1));

    // Decode neighbors' forward flow (channels R and G)
    let u_fwd_avg = (decode(f_m0.r, max_d) + decode(f_p0.r, max_d) +
                     decode(f_0m.r, max_d) + decode(f_0p.r, max_d)) * 0.25;
    let v_fwd_avg = (decode(f_m0.g, max_d) + decode(f_p0.g, max_d) +
                     decode(f_0m.g, max_d) + decode(f_0p.g, max_d)) * 0.25;

    // Horn-Schunck update for forward flow:
    //   u_new = u_avg - Ix * (Ix*u_avg + Iy*v_avg + It) / (alpha^2 + Ix^2 + Iy^2)
    let denom_fwd = alpha * alpha + Ix * Ix + Iy * Iy;
    let numerator_fwd = Ix * u_fwd_avg + Iy * v_fwd_avg + It;
    let u_fwd_new = u_fwd_avg - Ix * numerator_fwd / denom_fwd;
    let v_fwd_new = v_fwd_avg - Iy * numerator_fwd / denom_fwd;

    // --- Backward flow: same logic but with negated It (B -> A direction) ---
    let b_mm = load_luma(luma_b, px + vec2<i32>(-1, -1));
    let b_0m = load_luma(luma_b, px + vec2<i32>( 0, -1));
    let b_pm = load_luma(luma_b, px + vec2<i32>( 1, -1));
    let b_m0 = load_luma(luma_b, px + vec2<i32>(-1,  0));
    let b_p0 = load_luma(luma_b, px + vec2<i32>( 1,  0));
    let b_mp = load_luma(luma_b, px + vec2<i32>(-1,  1));
    let b_0p = load_luma(luma_b, px + vec2<i32>( 0,  1));
    let b_pp = load_luma(luma_b, px + vec2<i32>( 1,  1));

    let Bx = (-b_mm + b_pm - 2.0*b_m0 + 2.0*b_p0 - b_mp + b_pp) / 8.0;
    let By = (-b_mm - 2.0*b_0m - b_pm + b_mp + 2.0*b_0p + b_pp) / 8.0;
    let Bt = -It; // reversed temporal direction

    let u_bwd_avg = (decode(f_m0.b, max_d) + decode(f_p0.b, max_d) +
                     decode(f_0m.b, max_d) + decode(f_0p.b, max_d)) * 0.25;
    let v_bwd_avg = (decode(f_m0.a, max_d) + decode(f_p0.a, max_d) +
                     decode(f_0m.a, max_d) + decode(f_0p.a, max_d)) * 0.25;

    let denom_bwd = alpha * alpha + Bx * Bx + By * By;
    let numerator_bwd = Bx * u_bwd_avg + By * v_bwd_avg + Bt;
    let u_bwd_new = u_bwd_avg - Bx * numerator_bwd / denom_bwd;
    let v_bwd_new = v_bwd_avg - By * numerator_bwd / denom_bwd;

    // Clamp before encoding
    let clamp_range = max_d;
    let u_fwd_c = clamp(u_fwd_new, -clamp_range, clamp_range);
    let v_fwd_c = clamp(v_fwd_new, -clamp_range, clamp_range);
    let u_bwd_c = clamp(u_bwd_new, -clamp_range, clamp_range);
    let v_bwd_c = clamp(v_bwd_new, -clamp_range, clamp_range);

    let out = vec4<f32>(
        encode(u_fwd_c, max_d),
        encode(v_fwd_c, max_d),
        encode(u_bwd_c, max_d),
        encode(v_bwd_c, max_d),
    );

    textureStore(flow_out, px, out);
}

