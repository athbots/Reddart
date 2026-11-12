//! # rd-render: Render Graph & GPU Compositor
//!
//! Compiles the timeline into a render graph DAG, manages result caching,
//! dirty-node invalidation, and the adaptive quality governor.

pub mod cache;
pub mod composite;
pub mod compositor;
pub mod graph;
pub mod interpolate;
pub mod optical_flow;
pub mod quality;

pub use composite::{CompositorLayer, composite_layers_over};
pub use interpolate::{blend_rgba_frames, interpolate_optical_flow, interpolate_retimed_frame};
pub use optical_flow::{DenseOpticalFlowEstimator, OpticalFlowConfig, OpticalFlowField};

/// Render graph node identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NodeId(pub u64);
