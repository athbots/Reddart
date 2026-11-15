# Reddart Nested Timelines & Dedicated Sub-Sequence Caching Architecture

## 1. Executive Summary & Recursion Topology

Reddart Phase II implements multi-level recursive nested timelines (compound sequences) coupled with a dedicated sub-sequence pre-render caching layer (`rd_render::SubSequenceCache`). This subsystem enables complex hierarchical video compositions—such as nested lower-thirds, multi-track VFX sequences, and animated titles—while eliminating redundant graph compilation and evaluation overhead during timeline playback and scrubbing.

```
                          ┌────────────────────────┐
                          │   Main Timeline (L0)   │
                          │   Track V1: Background │
                          │   Track V2: Compound A │
                          └───────────┬────────────┘
                                      │
                               time_us (L0)
                                      │
                     SubSequenceCache::get(key)?
                                      │
             ┌────────────────────────┴────────────────────────┐
             ▼ (Hit)                                           ▼ (Miss)
 ┌───────────────────────┐                         ┌───────────────────────┐
 │ NodeKind::            │                         │ Recursive Compile:    │
 │ CachedSequence        │                         │ Inner Timeline (L1)   │
 │ (Pre-rendered buffer) │                         │ Track V1: Compound B  │
 └───────────────────────┘                         └───────────┬───────────┘
                                                               │
                                                       time_us * speed_1
                                                               │
                                                   SubSequenceCache::get(key)?
                                                               │
                                       ┌───────────────────────┴───────────────────────┐
                                       ▼ (Hit)                                         ▼ (Miss)
                           ┌───────────────────────┐                       ┌───────────────────────┐
                           │ NodeKind::            │                       │ Recursive Compile:    │
                           │ CachedSequence        │                       │ Inner Timeline (L2)   │
                           │ (Pre-rendered buffer) │                       │ Leaf Source Clips     │
                           └───────────────────────┘                       └───────────────────────┘
```

---

## 2. Deep Multi-Level Recursion & Speed Cascades

### 2.1 Arbitrary-Depth Sequence Nesting
Compound clips in Reddart wrap an isolated `rd_core::Timeline` inside a standard clip container. When compiling a project into a `RenderGraph`, the engine recursively traverses nested compound sequences to arbitrary depths.

### 2.2 Temporal Coordinate & Speed Propagation
For any nested clip at recursion depth $k$, the local source timestamp is determined by the cumulative product of all parent compound clip playback speeds:

$$t_{\text{source}}^{(k)} = \left( t_{\text{parent}} - t_{\text{start}} \right) \cdot \prod_{i=0}^{k} s_i$$

- **Speed Ramps & Time Remapping:** Changing the outer compound clip's speed (e.g. $2.0\times$ or $0.5\times$) instantly shifts the evaluation coordinates of all inner clips without modifying internal keyframe schedules.
- **Bound Clamping:** Source times outside the active compound clip boundary $(0 \le t_{\text{source}} < \text{duration})$ are excluded from the render graph, ensuring zero CPU/GPU overhead for dormant sequences.

---

## 3. Dedicated Sub-Sequence Caching Layer (`SubSequenceCache`)

### 3.1 Cache Key Formulation
To guarantee cache safety and deterministic invalidation across edits, cache entries are uniquely identified by three orthogonal attributes:

```rust
pub struct SubSequenceCacheKey {
    pub compound_clip_id: rd_core::ClipId,
    pub source_time_us: rd_core::TimestampUs,
    pub sequence_hash: SequenceHash,
}
```

- `compound_clip_id`: Identifies the parent compound instance on the track.
- `source_time_us`: The exact temporal evaluation point inside the sub-sequence.
- `sequence_hash`: A 64-bit deterministic hash of all inner tracks, clip intervals, speeds, keyframes, and nested sub-timelines.

### 3.2 O(1) DAG Node Substitution
When `RenderGraph::compile_timeline_with_cache` encounters a compound clip whose `SubSequenceCacheKey` exists in the cache:
1. The engine bypasses recursive descent into the inner timeline tracks and clips.
2. It emits a single `NodeKind::CachedSequence` node representing the pre-rendered frame.
3. Outer 2D transformations (translation, scale, rotation) and effects are applied directly to the cached node.
4. **Performance Impact:** Reduces DAG compilation overhead from $O(M)$ inner operations to $O(1)$, maintaining sub-microsecond compilation latencies even with hundreds of nested video layers.

### 3.3 Fine-Grained Cache Invalidation
- **Clip Mutation:** Modifying any clip or keyframe inside the compound sequence alters its `content_hash()`, automatically creating a new `SubSequenceCacheKey` and forcing a clean cache miss.
- **Selective Eviction:** `cache.invalidate_clip(compound_id)` purges all cached frames associated with a specific compound clip without flushing unrelated timeline cache entries.
- **LRU Memory Management:** In-memory cached frames are bounded by a configurable capacity (default: 128 frames) using an access tick counter for least-recently-used eviction.

---

## 4. Verification & Test Evidence

The sub-sequence caching and recursion subsystems are validated by dedicated integration tests in `crates/rd-render/tests/nested_sequence_render_test.rs`:

| Test Name | Verification Target | Status |
|---|---|---|
| `test_compound_clip_dag_compilation_and_hierarchy` | Single-level compound clip DAG topology and source count | Passed |
| `test_compound_clip_software_compositing_pipeline` | Bit-accurate alpha compositing of compound layers | Passed |
| `test_compound_clip_speed_remapping` | Temporal speed scaling and source timestamp mapping | Passed |
| `test_multi_level_deep_recursion_3_levels` | 3-level deep sequence nesting with speed multiplying cascade | Passed |
| `test_sub_sequence_cache_hit_and_invalidation` | $O(1)$ `CachedSequence` substitution and mutation invalidation | Passed |
| `test_sub_sequence_cache_eviction_lru` | Strict LRU eviction order under memory capacity bounds | Passed |
