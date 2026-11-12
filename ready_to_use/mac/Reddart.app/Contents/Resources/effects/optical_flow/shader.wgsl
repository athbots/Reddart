// Optical Flow Motion-Compensated Retiming & Slow Motion Shader
// Bidirectionally warps between Frame A and Frame B based on dense motion vector fields

struct OpticalFlowParams {
    retime_speed: f32,          // Playback speed ratio (0.5 = 2x slow motion)
    blend_time: f32,            // Normalized temporal phase [0.0, 1.0] between Frame A and Frame B
    flow_smoothness: f32,       // Spatial flow regularization factor [0.0, 1.0]
    occlusion_suppression: f32, // Bilateral occlusion weight [0.0, 1.0]
}

@group(0) @binding(0) var frame_a_texture: texture_2d<f32>;
@group(0) @binding(1) var frame_b_texture: texture_2d<f32>;
@group(0) @binding(2) var flow_texture: texture_2d<f32>; // RG = Forward flow (u, v), BA = Backward flow (u', v')
@group(0) @binding(3) var tex_sampler: sampler;
@group(0) @binding(4) var<uniform> params: OpticalFlowParams;

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
    let t = clamp(params.blend_time, 0.0, 1.0);
    let inv_t = 1.0 - t;

    // Sample motion vectors: RG = Forward flow, BA = Backward flow
    let flow_sample = textureSample(flow_texture, tex_sampler, in.tex_coords);
    let u_fwd = (flow_sample.r - 0.5) * 2.0 * params.flow_smoothness;
    let v_fwd = (flow_sample.g - 0.5) * 2.0 * params.flow_smoothness;
    let u_bwd = (flow_sample.b - 0.5) * 2.0 * params.flow_smoothness;
    let v_bwd = (flow_sample.a - 0.5) * 2.0 * params.flow_smoothness;

    // Forward-backward motion vector symmetry
    let motion = vec2<f32>(0.5 * (u_fwd - u_bwd), 0.5 * (v_fwd - v_bwd));

    // Warped coordinates
    let coord_a = clamp(in.tex_coords - t * motion, vec2<f32>(0.0), vec2<f32>(1.0));
    let coord_b = clamp(in.tex_coords + inv_t * motion, vec2<f32>(0.0), vec2<f32>(1.0));

    let sample_a = textureSample(frame_a_texture, tex_sampler, coord_a);
    let sample_b = textureSample(frame_b_texture, tex_sampler, coord_b);

    // Forward-backward consistency check for occlusion estimation
    let roundtrip_err = length(vec2<f32>(u_fwd + u_bwd, v_fwd + v_bwd));
    let occlusion = clamp(roundtrip_err * params.occlusion_suppression, 0.0, 1.0);

    // Adaptive blending
    let weight_a = max(inv_t * (1.0 - occlusion * t), 0.0);
    let weight_b = max(t * (1.0 - occlusion * inv_t), 0.0);
    let total_weight = max(weight_a + weight_b, 0.0001);

    let final_color = (sample_a * weight_a + sample_b * weight_b) / total_weight;
    return final_color;
}

