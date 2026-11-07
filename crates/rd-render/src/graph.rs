//! Render Graph: DAG of GPU rendering operations compiled from the timeline.
//!
//! Section 3.4 of the Technical Proposal:
//! The timeline compiles into an acyclic directed graph (DAG).
//! - Nodes are Decode, Effect, Transform, Blend, and Output.
//! - Each node computes a deterministic 64-bit content hash of its inputs and parameters.
//! - Dirty tracking invalidates only changed subtrees when parameters change.
//! - Topological sorting ensures execution order.

use std::collections::{HashMap, HashSet, VecDeque};
use std::hash::{DefaultHasher, Hash, Hasher};

use crate::NodeId;
use rd_core::{ClipId, TimestampUs, TrackId};

/// Node types in the render DAG.
#[derive(Debug, Clone)]
pub enum NodeKind {
    /// Source frame from a decoded video, image, or generated text.
    Source {
        clip_id: ClipId,
        track_id: TrackId,
        source_time_us: TimestampUs,
    },
    /// Shader effect pass (e.g. brightness, LUT, blur).
    Effect { effect_id: String, params_hash: u64 },
    /// 2D Transform / spatial positioning (scale, translation, rotation, crop).
    Transform {
        x: f32,
        y: f32,
        scale_x: f32,
        scale_y: f32,
        rotation_degrees: f32,
        opacity: f32,
    },
    /// Blend two layers together (base + overlay) using a blend mode.
    Blend { blend_mode: u32, opacity: f32 },
    /// Final output target (screen preview or encoder).
    Output { width: u32, height: u32 },
}

/// A node in the render graph.
#[derive(Debug, Clone)]
pub struct RenderNode {
    pub id: NodeId,
    pub kind: NodeKind,
    /// Incoming dependency inputs (node IDs whose outputs are consumed by this node).
    pub inputs: Vec<NodeId>,
    /// Precomputed content hash combining node parameters and input hashes.
    pub content_hash: u64,
}

/// Render Graph: directed acyclic graph of compositing operations.
#[derive(Debug, Clone)]
pub struct RenderGraph {
    nodes: HashMap<NodeId, RenderNode>,
    output_node: Option<NodeId>,
    next_node_id: u64,
}

impl RenderGraph {
    pub fn new() -> Self {
        Self {
            nodes: HashMap::new(),
            output_node: None,
            next_node_id: 1,
        }
    }

    /// Allocate a unique NodeId.
    fn alloc_id(&mut self) -> NodeId {
        let id = NodeId(self.next_node_id);
        self.next_node_id += 1;
        id
    }

    /// Add a source frame node.
    pub fn add_source(
        &mut self,
        clip_id: ClipId,
        track_id: TrackId,
        source_time_us: TimestampUs,
    ) -> NodeId {
        let id = self.alloc_id();
        let mut hasher = DefaultHasher::new();
        "source".hash(&mut hasher);
        clip_id.0.hash(&mut hasher);
        track_id.0.hash(&mut hasher);
        source_time_us.hash(&mut hasher);
        let content_hash = hasher.finish();

        let node = RenderNode {
            id,
            kind: NodeKind::Source {
                clip_id,
                track_id,
                source_time_us,
            },
            inputs: Vec::new(),
            content_hash,
        };
        self.nodes.insert(id, node);
        id
    }

    /// Add an effect shader pass node consuming an input node.
    pub fn add_effect(&mut self, input: NodeId, effect_id: &str, params_hash: u64) -> NodeId {
        let id = self.alloc_id();
        let input_hash = self.nodes.get(&input).map(|n| n.content_hash).unwrap_or(0);

        let mut hasher = DefaultHasher::new();
        "effect".hash(&mut hasher);
        input_hash.hash(&mut hasher);
        effect_id.hash(&mut hasher);
        params_hash.hash(&mut hasher);
        let content_hash = hasher.finish();

        let node = RenderNode {
            id,
            kind: NodeKind::Effect {
                effect_id: effect_id.to_string(),
                params_hash,
            },
            inputs: vec![input],
            content_hash,
        };
        self.nodes.insert(id, node);
        id
    }

    /// Add a 2D transform node.
    #[allow(clippy::too_many_arguments)]
    pub fn add_transform(
        &mut self,
        input: NodeId,
        x: f32,
        y: f32,
        scale_x: f32,
        scale_y: f32,
        rotation_degrees: f32,
        opacity: f32,
    ) -> NodeId {
        let id = self.alloc_id();
        let input_hash = self.nodes.get(&input).map(|n| n.content_hash).unwrap_or(0);

        let mut hasher = DefaultHasher::new();
        "transform".hash(&mut hasher);
        input_hash.hash(&mut hasher);
        x.to_bits().hash(&mut hasher);
        y.to_bits().hash(&mut hasher);
        scale_x.to_bits().hash(&mut hasher);
        scale_y.to_bits().hash(&mut hasher);
        rotation_degrees.to_bits().hash(&mut hasher);
        opacity.to_bits().hash(&mut hasher);
        let content_hash = hasher.finish();

        let node = RenderNode {
            id,
            kind: NodeKind::Transform {
                x,
                y,
                scale_x,
                scale_y,
                rotation_degrees,
                opacity,
            },
            inputs: vec![input],
            content_hash,
        };
        self.nodes.insert(id, node);
        id
    }

    /// Add a blend node that composites an overlay over a base layer.
    pub fn add_blend(
        &mut self,
        base: NodeId,
        overlay: NodeId,
        blend_mode: u32,
        opacity: f32,
    ) -> NodeId {
        let id = self.alloc_id();
        let base_hash = self.nodes.get(&base).map(|n| n.content_hash).unwrap_or(0);
        let overlay_hash = self
            .nodes
            .get(&overlay)
            .map(|n| n.content_hash)
            .unwrap_or(0);

        let mut hasher = DefaultHasher::new();
        "blend".hash(&mut hasher);
        base_hash.hash(&mut hasher);
        overlay_hash.hash(&mut hasher);
        blend_mode.hash(&mut hasher);
        opacity.to_bits().hash(&mut hasher);
        let content_hash = hasher.finish();

        let node = RenderNode {
            id,
            kind: NodeKind::Blend {
                blend_mode,
                opacity,
            },
            inputs: vec![base, overlay],
            content_hash,
        };
        self.nodes.insert(id, node);
        id
    }

    /// Set the output target node.
    pub fn set_output(&mut self, input: NodeId, width: u32, height: u32) -> NodeId {
        let id = self.alloc_id();
        let input_hash = self.nodes.get(&input).map(|n| n.content_hash).unwrap_or(0);

        let mut hasher = DefaultHasher::new();
        "output".hash(&mut hasher);
        input_hash.hash(&mut hasher);
        width.hash(&mut hasher);
        height.hash(&mut hasher);
        let content_hash = hasher.finish();

        let node = RenderNode {
            id,
            kind: NodeKind::Output { width, height },
            inputs: vec![input],
            content_hash,
        };
        self.nodes.insert(id, node);
        self.output_node = Some(id);
        id
    }

    /// Retrieve a node by ID.
    pub fn get_node(&self, id: NodeId) -> Option<&RenderNode> {
        self.nodes.get(&id)
    }

    /// Get the graph's output node ID.
    pub fn output_node(&self) -> Option<NodeId> {
        self.output_node
    }

    /// Return nodes in topological order (dependencies execute before consumers).
    /// Uses Kahn's algorithm.
    pub fn topological_sort(&self) -> Result<Vec<NodeId>, &'static str> {
        let mut in_degree: HashMap<NodeId, usize> = HashMap::new();
        let mut dependents: HashMap<NodeId, Vec<NodeId>> = HashMap::new();

        for (&id, node) in &self.nodes {
            in_degree.entry(id).or_insert(0);
            for &input in &node.inputs {
                dependents.entry(input).or_default().push(id);
                *in_degree.entry(id).or_insert(0) += 1;
            }
        }

        let mut queue: VecDeque<NodeId> = in_degree
            .iter()
            .filter(|&(_, &deg)| deg == 0)
            .map(|(&id, _)| id)
            .collect();

        let mut sorted = Vec::with_capacity(self.nodes.len());
        let mut visited = HashSet::new();

        while let Some(id) = queue.pop_front() {
            sorted.push(id);
            visited.insert(id);

            if let Some(children) = dependents.get(&id) {
                for &child in children {
                    let deg = in_degree.get_mut(&child).unwrap();
                    *deg -= 1;
                    if *deg == 0 {
                        queue.push_back(child);
                    }
                }
            }
        }

        if sorted.len() != self.nodes.len() {
            Err("Cycle detected in render graph")
        } else {
            Ok(sorted)
        }
    }

    /// Compile a Project Timeline at a specific timestamp into a complete RenderGraph.
    pub fn compile_timeline(project: &rd_core::Project, time_us: TimestampUs) -> Self {
        let mut graph = Self::new();
        let mut active_layer_nodes = Vec::new();

        // Examine active tracks from bottom to top
        for track in project.timeline.tracks() {
            // Find active clip on this track at time_us
            if let Some(clip) = track.clip_at(time_us) {
                // Map timeline timestamp to clip source timestamp
                if let Some(source_time) = clip.timeline_to_source_us(time_us) {
                    // 1. Source node
                    let mut current = graph.add_source(clip.id, track.id, source_time);

                    // 2. Transform node with keyframe evaluation
                    let eff_transform = clip.effective_transform(time_us);
                    let eff_opacity = clip.effective_opacity(time_us);
                    current = graph.add_transform(
                        current,
                        eff_transform.position.x,
                        eff_transform.position.y,
                        eff_transform.scale.x,
                        eff_transform.scale.y,
                        eff_transform.rotation,
                        eff_opacity,
                    );

                    // 3. Effects stack with keyframe evaluation
                    for effect in &clip.effects {
                        if effect.enabled {
                            let mut param_hasher = DefaultHasher::new();
                            for (k, v) in &effect.parameters {
                                k.hash(&mut param_hasher);
                                let val = clip.evaluate_keyframe(k, time_us).unwrap_or(*v);
                                val.to_bits().hash(&mut param_hasher);
                            }
                            current =
                                graph.add_effect(current, &effect.effect_id, param_hasher.finish());
                        }
                    }

                    active_layer_nodes.push(current);
                }
            }
        }

        if active_layer_nodes.is_empty() {
            // Empty frame: output empty 0-layer graph
            let empty_src = graph.add_source(ClipId::new(), TrackId(0), 0);
            graph.set_output(
                empty_src,
                project.resolution.width,
                project.resolution.height,
            );
            return graph;
        }

        // Composite layers bottom-up using blend nodes
        let mut composite = active_layer_nodes[0];
        for &layer in &active_layer_nodes[1..] {
            // Blend mode 0 = Normal
            composite = graph.add_blend(composite, layer, 0, 1.0);
        }

        // Output node
        graph.set_output(
            composite,
            project.resolution.width,
            project.resolution.height,
        );

        graph
    }
}

impl Default for RenderGraph {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_topological_sort() {
        let mut graph = RenderGraph::new();
        let src1 = graph.add_source(ClipId::new(), TrackId(1), 0);
        let t1 = graph.add_transform(src1, 0.0, 0.0, 1.0, 1.0, 0.0, 1.0);
        let eff1 = graph.add_effect(t1, "brightness", 12345);

        let src2 = graph.add_source(ClipId::new(), TrackId(2), 0);
        let blend = graph.add_blend(eff1, src2, 0, 1.0);
        let out = graph.set_output(blend, 1920, 1080);

        let order = graph.topological_sort().expect("No cycle");
        assert_eq!(order.len(), 6);
        // Output must be last
        assert_eq!(*order.last().unwrap(), out);

        // Sources must appear before their consumers
        let src1_idx = order.iter().position(|&id| id == src1).unwrap();
        let t1_idx = order.iter().position(|&id| id == t1).unwrap();
        let eff1_idx = order.iter().position(|&id| id == eff1).unwrap();
        let blend_idx = order.iter().position(|&id| id == blend).unwrap();

        assert!(src1_idx < t1_idx);
        assert!(t1_idx < eff1_idx);
        assert!(eff1_idx < blend_idx);
    }

    #[test]
    fn test_content_hash_determinism_and_invalidation() {
        let cid = ClipId::new();
        let mut g1 = RenderGraph::new();
        let s1 = g1.add_source(cid, TrackId(1), 1_000_000);
        let e1 = g1.add_effect(s1, "contrast", 999);

        let mut g2 = RenderGraph::new();
        let s2 = g2.add_source(cid, TrackId(1), 1_000_000);
        let e2 = g2.add_effect(s2, "contrast", 999);

        // Same parameters must yield identical content hashes
        assert_eq!(
            g1.get_node(e1).unwrap().content_hash,
            g2.get_node(e2).unwrap().content_hash
        );

        // Changing parameter must produce a different hash (dirty invalidation)
        let mut g3 = RenderGraph::new();
        let s3 = g3.add_source(cid, TrackId(1), 1_000_000);
        let e3 = g3.add_effect(s3, "contrast", 1000); // Changed param

        assert_ne!(
            g1.get_node(e1).unwrap().content_hash,
            g3.get_node(e3).unwrap().content_hash
        );
    }

    #[test]
    fn test_compile_timeline() {
        let mut project = rd_core::Project::new("Test Project");
        let v1 = project
            .timeline
            .add_track(rd_core::TrackKind::Video, "Video 1");
        let v2 = project
            .timeline
            .add_track(rd_core::TrackKind::Video, "Video 2");

        let clip1 = rd_core::Clip::new_video("clip1.mp4", 0, 0, 5_000_000);
        let clip2 = rd_core::Clip::new_video("clip2.mp4", 1_000_000, 0, 3_000_000);

        project.timeline.track_mut(v1).unwrap().add_clip(clip1);
        project.timeline.track_mut(v2).unwrap().add_clip(clip2);

        // At 2.0s, both clips are active -> graph should have 2 layers + blend + output
        let graph = RenderGraph::compile_timeline(&project, 2_000_000);
        let order = graph.topological_sort().expect("Valid DAG");
        assert!(order.len() >= 6); // 2 sources, 2 transforms, 1 blend, 1 output
    }
}
