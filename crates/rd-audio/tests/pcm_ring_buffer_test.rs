use std::sync::Arc;
use std::thread;
use std::time::Duration;

use rd_audio::{AudioClock, AudioEngine, PcmRingBuffer};

#[test]
fn test_pcm_ring_buffer_spsc_concurrency() {
    let capacity = 8192;
    let ring = Arc::new(PcmRingBuffer::new(capacity));
    let total_samples = 100_000;

    let ring_producer = ring.clone();
    let producer = thread::spawn(move || {
        let mut written_total = 0;
        let mut sample_val = 1.0f32;

        while written_total < total_samples {
            let avail = ring_producer.available_write();
            if avail == 0 {
                thread::yield_now();
                continue;
            }
            let to_write = avail.min(512).min(total_samples - written_total);
            let mut chunk = Vec::with_capacity(to_write);
            for _ in 0..to_write {
                chunk.push(sample_val);
                sample_val += 1.0;
            }

            let written = ring_producer.write(&chunk);
            assert_eq!(written, to_write);
            written_total += written;
        }
    });

    let ring_consumer = ring.clone();
    let consumer = thread::spawn(move || {
        let mut read_total = 0;
        let mut buf = vec![0.0f32; 256];
        let mut expected_val = 1.0f32;

        while read_total < total_samples {
            let avail = ring_consumer.available_read();
            if avail == 0 {
                thread::yield_now();
                continue;
            }
            let to_read = avail.min(buf.len()).min(total_samples - read_total);
            let n = ring_consumer.read_or_zero(&mut buf[..to_read]);
            for &sample in &buf[..n] {
                assert_eq!(
                    sample, expected_val,
                    "Sample sequence mismatch at index {}",
                    read_total
                );
                expected_val += 1.0;
            }
            read_total += n;
        }
    });

    producer.join().expect("Producer thread failed");
    consumer.join().expect("Consumer thread failed");
    assert_eq!(ring.available_read(), 0);
}

#[test]
fn test_pcm_ring_buffer_bounded_capacity_and_underrun_recovery() {
    let capacity = 1024;
    let ring = PcmRingBuffer::new(capacity);
    assert_eq!(ring.capacity(), capacity);
    assert_eq!(ring.available_read(), 0);
    assert_eq!(ring.available_write(), capacity);

    // Attempting to write more than capacity must bound and not overflow
    let large_chunk = vec![0.5f32; 2048];
    let written = ring.write(&large_chunk);
    assert_eq!(written, capacity);
    assert_eq!(ring.available_write(), 0);
    assert_eq!(ring.available_read(), capacity);

    // Further writes must return 0 without hanging
    let overflow_written = ring.write(&[1.0, 2.0, 3.0]);
    assert_eq!(overflow_written, 0);

    // Consumer reads 512 samples
    let mut out_half = vec![0.0f32; 512];
    let read_half = ring.read_or_zero(&mut out_half);
    assert_eq!(read_half, 512);
    assert!(out_half.iter().all(|&s| s == 0.5));
    assert_eq!(ring.available_read(), 512);
    assert_eq!(ring.available_write(), 512);

    // Read remaining 512 plus 256 extra (simulating driver underrun)
    let mut out_underrun = vec![99.0f32; 768];
    let read_count = ring.read_or_zero(&mut out_underrun);
    assert_eq!(read_count, 512);
    // Genuine samples are 0.5
    assert!(out_underrun[..512].iter().all(|&s| s == 0.5));
    // Underrun buffer tail must be cleanly zero-filled
    assert!(out_underrun[512..].iter().all(|&s| s == 0.0));
    assert_eq!(ring.available_read(), 0);
    assert_eq!(ring.available_write(), capacity);
}

#[test]
fn test_pcm_ring_buffer_instantaneous_atomic_clear() {
    let ring = PcmRingBuffer::new(2048);
    let chunk = vec![0.75f32; 1000];
    ring.write(&chunk);
    assert_eq!(ring.available_read(), 1000);

    // Instantaneous atomic flush (e.g. playhead seek or pause)
    ring.clear();
    assert_eq!(ring.available_read(), 0);
    assert_eq!(ring.available_write(), 2048);

    // Verify subsequent read produces clean zeroes
    let mut out = vec![42.0f32; 256];
    let read_count = ring.read_or_zero(&mut out);
    assert_eq!(read_count, 0);
    assert!(out.iter().all(|&s| s == 0.0));
}

#[test]
fn test_audio_engine_background_mixing_fills_ring_buffer() {
    let clock = AudioClock::new();
    let engine = AudioEngine::new(clock).expect("Failed to create AudioEngine");

    // Load synthetic test audio (1 second of 48 kHz stereo = 96,000 samples)
    let test_audio = vec![0.4f32; 96_000];
    engine.set_timeline_pcm(test_audio);
    assert_eq!(engine.ring_buffer().available_read(), 0);

    // Start playback: background mixer thread should wake up and begin filling the ring buffer
    engine.play();
    assert!(engine.is_playing());

    // Wait briefly for background mixer thread to populate the ring buffer
    let start = std::time::Instant::now();
    let mut buffer_populated = false;
    while start.elapsed() < Duration::from_millis(500) {
        if engine.ring_buffer().available_read() >= 1024 {
            buffer_populated = true;
            break;
        }
        thread::sleep(Duration::from_millis(10));
    }

    assert!(
        buffer_populated,
        "Background mixer thread must fill the bounded PCM ring buffer during playback"
    );

    // Verify consumer can pull samples from ring buffer without locking
    let mut driver_buf = vec![0.0f32; 512];
    let read = engine.ring_buffer().read_or_zero(&mut driver_buf);
    assert_eq!(read, 512);
    // Samples should reflect mixed track (0.4f32)
    assert!(
        driver_buf.iter().all(|&s| (s - 0.4).abs() < 1e-4),
        "Samples popped from ring buffer should match mixed track data"
    );

    // Pause playback: ring buffer should be cleared
    engine.pause();
    assert!(!engine.is_playing());
    assert_eq!(engine.ring_buffer().available_read(), 0);
}
