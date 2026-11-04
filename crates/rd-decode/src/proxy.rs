//! Proxy generation and caching: all-intra 540p proxies for zero-cost scrubbing.
//!
//! Section 3.1 of the Technical Proposal:
//! Normal video is long-GOP H.264/H.265. To show one frame, the decoder must
//! start at the previous keyframe and decode forward, making scrubbing sluggish.
//! On import, a background job creates a 540p all-intra (GOP=1) proxy. Every frame
//! is independently decodable, so random access is instantaneous and reverse playback
//! is cheap. The editor works on proxies; export renders with originals.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Proxy generation configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProxyConfig {
    /// Target vertical resolution (e.g. 540 for 540p).
    pub target_height: u32,
    /// Bitrate in kilobits per second.
    pub bitrate_kbps: u32,
    /// Maximum disk cache size in megabytes. Default: 5,000 MB (5 GB).
    pub max_cache_mb: u64,
}

impl Default for ProxyConfig {
    fn default() -> Self {
        Self {
            target_height: 540,
            bitrate_kbps: 4000, // 4 Mbps gives clean 540p preview
            max_cache_mb: 5000,
        }
    }
}

/// Status of proxy generation for a media file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProxyStatus {
    /// Proxy has not been requested or started yet.
    NotStarted,
    /// Proxy is actively generating in background (0.0 to 1.0 progress).
    Generating { progress_percent: u8 },
    /// Proxy is complete and available on disk.
    Ready { proxy_path: PathBuf },
    /// Proxy generation failed with an error message.
    Failed { error: String },
}

/// Proxy cache manager managing generated proxy files on disk.
pub struct ProxyManager {
    config: ProxyConfig,
    cache_dir: PathBuf,
}

impl ProxyManager {
    pub fn new(cache_dir: impl Into<PathBuf>, config: ProxyConfig) -> Self {
        let cache_dir = cache_dir.into();
        let _ = std::fs::create_dir_all(&cache_dir);
        Self { config, cache_dir }
    }

    /// Compute deterministic proxy file path from the source file path and hash.
    pub fn proxy_path_for(&self, source_path: &Path) -> PathBuf {
        let file_stem = source_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("media");

        let hash = {
            use std::hash::{DefaultHasher, Hash, Hasher};
            let mut hasher = DefaultHasher::new();
            source_path.to_string_lossy().hash(&mut hasher);
            hasher.finish()
        };

        let filename = format!("{}_{:016x}_proxy_540p.mp4", file_stem, hash);
        self.cache_dir.join(filename)
    }

    /// Check if a completed proxy already exists on disk.
    pub fn has_proxy(&self, source_path: &Path) -> bool {
        let path = self.proxy_path_for(source_path);
        path.exists()
    }

    /// Get proxy configuration.
    pub fn config(&self) -> &ProxyConfig {
        &self.config
    }

    /// Cache directory path.
    pub fn cache_dir(&self) -> &Path {
        &self.cache_dir
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_proxy_path_generation() {
        let dir = std::env::temp_dir().join("reddart_proxy_test");
        let manager = ProxyManager::new(&dir, ProxyConfig::default());
        let p1 = manager.proxy_path_for(Path::new("C:/videos/nature.mp4"));
        let p2 = manager.proxy_path_for(Path::new("C:/videos/nature.mp4"));
        assert_eq!(p1, p2);
        assert!(p1.to_string_lossy().contains("nature_"));
        assert!(p1.to_string_lossy().contains("proxy_540p.mp4"));
    }
}
