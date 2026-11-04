//! Decoder pool: manages hardware decoders with per-chipset concurrency caps.
//!
//! Section 3.2 of the Technical Proposal:
//! A decoder pool keeps only the decoders near the playhead warm, with a
//! per-chipset cap (2 on weak mobile GPUs, 4-8 on desktop).
//! Proxies mean even inactive layers are cheap to decode.
//! Decoder crashes are caught and the clip decoder is transparently re-created.

use crate::VideoDecoder;
use rd_core::ClipId;
use std::collections::HashMap;

/// Entry in the decoder pool with LRU access tracking.
struct PoolEntry {
    decoder: Box<dyn VideoDecoder>,
    last_access_tick: u64,
    #[allow(dead_code)]
    source_path: String,
}

/// Hardware decoder pool.
pub struct DecoderPool {
    /// Active warm decoders keyed by clip ID.
    decoders: HashMap<ClipId, PoolEntry>,
    /// Maximum concurrent hardware decoder instances allowed.
    max_instances: usize,
    /// Monotonic tick for LRU eviction.
    current_tick: u64,
}

impl DecoderPool {
    /// Create a new decoder pool with a concurrency limit.
    pub fn new(max_instances: usize) -> Self {
        Self {
            decoders: HashMap::new(),
            max_instances: max_instances.max(1),
            current_tick: 0,
        }
    }

    /// Access a warm decoder for a clip, or returns None if not in the pool.
    pub fn get_mut(&mut self, clip_id: ClipId) -> Option<&mut Box<dyn VideoDecoder>> {
        self.current_tick += 1;
        let tick = self.current_tick;
        if let Some(entry) = self.decoders.get_mut(&clip_id) {
            entry.last_access_tick = tick;
            Some(&mut entry.decoder)
        } else {
            None
        }
    }

    /// Insert or replace a decoder in the pool.
    /// If the pool exceeds `max_instances`, the least recently used decoder is evicted.
    pub fn insert(&mut self, clip_id: ClipId, source_path: String, decoder: Box<dyn VideoDecoder>) {
        self.current_tick += 1;

        // Evict LRU if at capacity
        while self.decoders.len() >= self.max_instances && !self.decoders.contains_key(&clip_id) {
            self.evict_oldest();
        }

        self.decoders.insert(
            clip_id,
            PoolEntry {
                decoder,
                last_access_tick: self.current_tick,
                source_path,
            },
        );
    }

    /// Evict the least recently used decoder.
    fn evict_oldest(&mut self) -> bool {
        let oldest_key = self
            .decoders
            .iter()
            .min_by_key(|(_, entry)| entry.last_access_tick)
            .map(|(id, _)| *id);

        if let Some(key) = oldest_key {
            log::info!("Decoder pool: evicting LRU decoder for clip {:?}", key);
            self.decoders.remove(&key);
            true
        } else {
            false
        }
    }

    /// Check if a decoder for a clip is currently warm.
    pub fn is_warm(&self, clip_id: ClipId) -> bool {
        self.decoders.contains_key(&clip_id)
    }

    /// Number of active warm decoders in the pool.
    pub fn active_count(&self) -> usize {
        self.decoders.len()
    }

    /// Maximum instances allowed.
    pub fn max_instances(&self) -> usize {
        self.max_instances
    }

    /// Remove a specific clip decoder (e.g. clip removed from timeline).
    pub fn remove(&mut self, clip_id: ClipId) {
        self.decoders.remove(&clip_id);
    }

    /// Flush all decoders (e.g. project close or memory pressure).
    pub fn clear(&mut self) {
        self.decoders.clear();
    }
}

impl Default for DecoderPool {
    fn default() -> Self {
        // Desktop default: 4 concurrent decoders
        Self::new(4)
    }
}
