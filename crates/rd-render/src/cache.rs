//! Result Cache: LRU texture cache keyed by 64-bit content hash.
//!
//! Section 3.4 of the Technical Proposal:
//! Rendered node outputs are cached by content hash. Pausing, scrubbing
//! back, and looping cost almost nothing because intermediate and final
//! frame textures are served directly from GPU cache.

use std::collections::HashMap;

/// 64-bit hash representing node inputs, parameters, and source frame.
pub type CacheKey = u64;

/// Cached GPU texture entry with LRU tracking and memory estimation.
pub struct CachedTexture {
    pub texture: wgpu::Texture,
    pub view: wgpu::TextureView,
    pub width: u32,
    pub height: u32,
    pub format: wgpu::TextureFormat,
    pub byte_size: usize,
    pub last_used_tick: u64,
}

/// LRU Texture Result Cache.
pub struct ResultCache {
    entries: HashMap<CacheKey, CachedTexture>,
    /// Access sequence counter for LRU eviction.
    current_tick: u64,
    /// Maximum allowed memory footprint in bytes.
    max_bytes: usize,
    /// Current estimated memory footprint in bytes.
    used_bytes: usize,
    /// Cache hits counter.
    hits: u64,
    /// Cache misses counter.
    misses: u64,
}

impl ResultCache {
    /// Create a new cache with a maximum memory capacity in bytes.
    /// E.g., 256MB on desktop, 128MB on mid-range phones.
    pub fn new(max_bytes: usize) -> Self {
        Self {
            entries: HashMap::new(),
            current_tick: 0,
            max_bytes,
            used_bytes: 0,
            hits: 0,
            misses: 0,
        }
    }

    /// Retrieve a cached texture by content hash.
    pub fn get(&mut self, key: CacheKey) -> Option<&CachedTexture> {
        self.current_tick += 1;
        let tick = self.current_tick;

        if let Some(entry) = self.entries.get_mut(&key) {
            entry.last_used_tick = tick;
            self.hits += 1;
            Some(entry)
        } else {
            self.misses += 1;
            None
        }
    }

    /// Insert a newly rendered texture into the cache.
    /// Evicts oldest entries if the insertion would exceed the memory budget.
    pub fn insert(
        &mut self,
        key: CacheKey,
        texture: wgpu::Texture,
        view: wgpu::TextureView,
        width: u32,
        height: u32,
        format: wgpu::TextureFormat,
    ) {
        self.current_tick += 1;
        // Estimate RGBA8 texture byte size: width * height * 4
        let byte_size = (width as usize) * (height as usize) * 4;

        // If replacing an existing entry for this key, subtract its size first
        if let Some(old) = self.entries.remove(&key) {
            self.used_bytes = self.used_bytes.saturating_sub(old.byte_size);
        }

        // Evict LRU entries until we fit within max_bytes
        while self.used_bytes + byte_size > self.max_bytes && !self.entries.is_empty() {
            if !self.evict_oldest() {
                break;
            }
        }

        self.entries.insert(
            key,
            CachedTexture {
                texture,
                view,
                width,
                height,
                format,
                byte_size,
                last_used_tick: self.current_tick,
            },
        );

        self.used_bytes += byte_size;
    }

    /// Evict the least recently used texture. Returns false if cache was empty.
    fn evict_oldest(&mut self) -> bool {
        let oldest_key = self
            .entries
            .iter()
            .min_by_key(|(_, entry)| entry.last_used_tick)
            .map(|(key, _)| *key);

        if let Some(key) = oldest_key
            && let Some(evicted) = self.entries.remove(&key)
        {
            self.used_bytes = self.used_bytes.saturating_sub(evicted.byte_size);
            return true;
        }

        false
    }

    /// Flush all cached entries (e.g. on low memory callback or project change).
    pub fn clear(&mut self) {
        self.entries.clear();
        self.used_bytes = 0;
    }

    /// Current cache hit ratio (0.0 to 1.0).
    pub fn hit_ratio(&self) -> f64 {
        let total = self.hits + self.misses;
        if total == 0 {
            0.0
        } else {
            self.hits as f64 / total as f64
        }
    }

    pub fn used_bytes(&self) -> usize {
        self.used_bytes
    }

    pub fn max_bytes(&self) -> usize {
        self.max_bytes
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// Content hash identifying the structural and parameter state of a nested sub-sequence.
pub type SequenceHash = u64;

/// Cache key uniquely identifying a sub-sequence frame at a specific source timestamp.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SubSequenceCacheKey {
    pub compound_clip_id: rd_core::ClipId,
    pub source_time_us: rd_core::TimestampUs,
    pub sequence_hash: SequenceHash,
}

/// In-memory pre-rendered sub-sequence cache entry (RGBA frame buffer).
#[derive(Debug, Clone)]
pub struct CachedSubSequenceFrame {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
    pub last_used_tick: u64,
}

/// Dedicated sub-sequence pre-render caching layer.
///
/// Prevents recursive re-evaluation of complex nested timeline graphs during scrubbing
/// and playback by caching rendered intermediate compound sequence frames.
#[derive(Debug)]
pub struct SubSequenceCache {
    frames: HashMap<SubSequenceCacheKey, CachedSubSequenceFrame>,
    current_tick: u64,
    max_frames: usize,
    hits: u64,
    misses: u64,
}

impl SubSequenceCache {
    /// Create a new sub-sequence cache with maximum capacity in number of frames.
    pub fn new(max_frames: usize) -> Self {
        Self {
            frames: HashMap::new(),
            current_tick: 0,
            max_frames: max_frames.max(1),
            hits: 0,
            misses: 0,
        }
    }

    /// Check if the cache contains a pre-rendered frame for the given key.
    pub fn contains(&self, key: &SubSequenceCacheKey) -> bool {
        self.frames.contains_key(key)
    }

    /// Retrieve a pre-rendered frame from cache by key.
    pub fn get(&mut self, key: &SubSequenceCacheKey) -> Option<&CachedSubSequenceFrame> {
        self.current_tick += 1;
        let tick = self.current_tick;
        if let Some(entry) = self.frames.get_mut(key) {
            entry.last_used_tick = tick;
            self.hits += 1;
            Some(entry)
        } else {
            self.misses += 1;
            None
        }
    }

    /// Insert a pre-rendered frame into the cache.
    pub fn insert(&mut self, key: SubSequenceCacheKey, width: u32, height: u32, rgba: Vec<u8>) {
        self.current_tick += 1;
        if self.frames.len() >= self.max_frames && !self.frames.contains_key(&key) {
            self.evict_oldest();
        }

        self.frames.insert(
            key,
            CachedSubSequenceFrame {
                width,
                height,
                rgba,
                last_used_tick: self.current_tick,
            },
        );
    }

    /// Evict the least recently used cached frame.
    fn evict_oldest(&mut self) -> bool {
        let oldest_key = self
            .frames
            .iter()
            .min_by_key(|(_, entry)| entry.last_used_tick)
            .map(|(key, _)| *key);

        if let Some(key) = oldest_key {
            self.frames.remove(&key);
            true
        } else {
            false
        }
    }

    /// Invalidate all cached entries for a specific compound clip.
    pub fn invalidate_clip(&mut self, clip_id: rd_core::ClipId) {
        self.frames.retain(|key, _| key.compound_clip_id != clip_id);
    }

    /// Clear all cached frames.
    pub fn clear(&mut self) {
        self.frames.clear();
        self.hits = 0;
        self.misses = 0;
    }

    pub fn len(&self) -> usize {
        self.frames.len()
    }

    pub fn is_empty(&self) -> bool {
        self.frames.is_empty()
    }

    pub fn hits(&self) -> u64 {
        self.hits
    }

    pub fn misses(&self) -> u64 {
        self.misses
    }
}

impl Default for SubSequenceCache {
    fn default() -> Self {
        Self::new(128)
    }
}
