//! Bounded lock-free Single-Producer Single-Consumer (SPSC) PCM audio ring buffer.
//!
//! Technical Proposal Section 3.5:
//! Offloads timeline audio mixing and multi-track sample summation from the real-time CPAL
//! audio hardware callback to a dedicated background mixing thread. The CPAL hardware callback
//! consumes from this bounded lock-free ring buffer with zero mutex contention, zero memory
//! allocations, and zero system calls.

use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};

/// Lock-free atomic representation of an `f32`.
#[derive(Debug)]
pub struct AtomicF32 {
    bits: AtomicU32,
}

impl AtomicF32 {
    /// Create a new atomic `f32`.
    pub fn new(val: f32) -> Self {
        Self {
            bits: AtomicU32::new(val.to_bits()),
        }
    }

    /// Load the `f32` value with relaxed ordering.
    #[inline]
    pub fn load(&self) -> f32 {
        f32::from_bits(self.bits.load(Ordering::Relaxed))
    }

    /// Store an `f32` value with relaxed ordering.
    #[inline]
    pub fn store(&self, val: f32) {
        self.bits.store(val.to_bits(), Ordering::Relaxed);
    }
}

/// Bounded lock-free Single-Producer Single-Consumer (SPSC) PCM audio ring buffer.
pub struct PcmRingBuffer {
    buffer: Box<[AtomicF32]>,
    capacity: usize,
    write_idx: AtomicUsize,
    read_idx: AtomicUsize,
}

impl PcmRingBuffer {
    /// Create a new bounded lock-free PCM ring buffer with fixed sample capacity.
    pub fn new(capacity: usize) -> Self {
        let capacity = capacity.max(2);
        let mut samples = Vec::with_capacity(capacity);
        for _ in 0..capacity {
            samples.push(AtomicF32::new(0.0));
        }
        Self {
            buffer: samples.into_boxed_slice(),
            capacity,
            write_idx: AtomicUsize::new(0),
            read_idx: AtomicUsize::new(0),
        }
    }

    /// Total sample capacity of the ring buffer.
    #[inline]
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// Number of samples currently available for consumption.
    #[inline]
    pub fn available_read(&self) -> usize {
        let w = self.write_idx.load(Ordering::Acquire);
        let r = self.read_idx.load(Ordering::Relaxed);
        w.wrapping_sub(r).min(self.capacity)
    }

    /// Space (in samples) currently available for writing.
    #[inline]
    pub fn available_write(&self) -> usize {
        self.capacity.saturating_sub(self.available_read())
    }

    /// Producer: write interleaved PCM samples into the ring buffer.
    /// Returns the number of samples actually written (up to available_write).
    /// Non-blocking, lock-free, zero heap allocation.
    pub fn write(&self, samples: &[f32]) -> usize {
        if samples.is_empty() {
            return 0;
        }
        let space = self.available_write();
        let to_write = samples.len().min(space);
        if to_write == 0 {
            return 0;
        }

        let w = self.write_idx.load(Ordering::Relaxed);
        for (i, &sample) in samples[..to_write].iter().enumerate() {
            let slot = (w.wrapping_add(i)) % self.capacity;
            self.buffer[slot].store(sample);
        }

        self.write_idx
            .store(w.wrapping_add(to_write), Ordering::Release);
        to_write
    }

    /// Consumer: read interleaved PCM samples from the ring buffer.
    /// If fewer samples are available than requested (underrun), remaining elements of `out` are filled with 0.0.
    /// Returns the number of genuine samples read.
    /// Non-blocking, lock-free, zero heap allocation.
    pub fn read_or_zero(&self, out: &mut [f32]) -> usize {
        if out.is_empty() {
            return 0;
        }
        let avail = self.available_read();
        let to_read = out.len().min(avail);

        let r = self.read_idx.load(Ordering::Relaxed);
        for (i, slot_out) in out[..to_read].iter_mut().enumerate() {
            let slot = (r.wrapping_add(i)) % self.capacity;
            *slot_out = self.buffer[slot].load();
        }

        if to_read < out.len() {
            out[to_read..].fill(0.0);
        }

        self.read_idx
            .store(r.wrapping_add(to_read), Ordering::Release);
        to_read
    }

    /// Reset read and write cursors instantaneously, discarding all pending samples.
    pub fn clear(&self) {
        let w = self.write_idx.load(Ordering::Acquire);
        self.read_idx.store(w, Ordering::Release);
    }
}
