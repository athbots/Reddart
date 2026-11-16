//! # rd-render: Render Graph & GPU Compositor
//!
//! Compiles the timeline into a render graph DAG, manages result caching,
//! dirty-node invalidation, and the adaptive quality governor.

pub mod cache;
pub mod composite;
pub mod compositor;
pub mod graph;
pub mod interpolate;
pub mod multi_gpu;
pub mod optical_flow;
pub mod quality;

pub use cache::{CachedSubSequenceFrame, SubSequenceCache, SubSequenceCacheKey};
pub use composite::{CompositorLayer, composite_layers_over};
pub use compositor::{Compositor, OpticalFlowUniforms, TextSdfUniforms};
pub use interpolate::{blend_rgba_frames, interpolate_optical_flow, interpolate_retimed_frame};
pub use multi_gpu::{
    GpuAdapterProfile, GpuNodeRole, InterconnectType, MultiGpuConfig, MultiGpuError,
    MultiGpuManager, TileDescriptor, WorkloadPartitionStrategy,
};
pub use optical_flow::{
    DenseOpticalFlowEstimator, GpuOpticalFlowConfig, GpuOpticalFlowEstimator, OpticalFlowConfig,
    OpticalFlowField,
};
pub use rd_effects::color_space;
pub use rd_effects::color_space::{
    AscCdlParams, ColorGamut, ColorPipeline, ColorPipelineConfig, Mat3, TransferFunction,
    aces_filmic_tone_map, decode_transfer_function, encode_transfer_function,
    gamut_transform_matrix,
};

/// Render graph node identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NodeId(pub u64);
