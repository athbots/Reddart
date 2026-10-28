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

        if let Some(key) = oldest_key {
            if let Some(evicted) = self.entries.remove(&key) {
                self.used_bytes = self.used_bytes.saturating_sub(evicted.byte_size);
                return true;
            }
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

