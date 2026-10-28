//! # rd-render: Render Graph & GPU Compositor
//!
//! Compiles the timeline into a render graph DAG, manages result caching,
//! dirty-node invalidation, and the adaptive quality governor.

pub mod cache;
pub mod compositor;
pub mod graph;
pub mod interpolate;
pub mod quality;

pub use interpolate::blend_rgba_frames;

/// Render graph node identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NodeId(pub u64);
