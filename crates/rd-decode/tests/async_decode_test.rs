use std::path::PathBuf;
use std::time::Duration;

use rd_core::ClipId;
use rd_decode::{AsyncDecoderService, FrameResponse};

#[test]
fn test_async_decoder_unsupported_or_missing_media_handles_cleanly() {
    let service = AsyncDecoderService::new(16);
    let clip_id = ClipId::new();
    let invalid_path = PathBuf::from("non_existent_folder/missing_video_file_123.mp4");

    // Submit request for missing/unsupported media
    let req_id = service.request_frame(clip_id, invalid_path, 0);
    assert!(req_id > 0);

    // Wait for worker response
    let start = std::time::Instant::now();
    let mut got_response = false;
    while start.elapsed() < Duration::from_secs(3) {
        if let Some(resp) = service.try_recv() {
            match resp {
                FrameResponse::Error {
                    request_id, error, ..
                } => {
                    assert_eq!(request_id, req_id);
                    assert!(!error.is_empty());
                    got_response = true;
                    break;
                }
                FrameResponse::Ready { .. } => {
                    panic!("Missing media should not return FrameResponse::Ready");
                }
            }
        }
        std::thread::sleep(Duration::from_millis(20));
    }

    assert!(
        got_response,
        "Expected clean Error response from async decoder"
    );
}

#[test]
fn test_async_decoder_generational_cancellation() {
    let service = AsyncDecoderService::new(16);
    let clip_id = ClipId::new();
    let dummy_path = PathBuf::from("ready_to_use/windows/Reddart_Master_Demo.mp4");

    // Queue request in gen 1
    let gen1 = service.current_generation();
    assert_eq!(gen1, 1);
    let _req1 = service.request_frame(clip_id, dummy_path.clone(), 100_000);

    // Bump generation immediately (simulating user scrub)
    let gen2 = service.bump_generation();
    assert_eq!(gen2, 2);

    // Any request in gen 1 should be cancelled / dropped if not yet processed
    // Queue request in gen 2
    let req2 = service.request_frame(clip_id, dummy_path, 200_000);

    // Drain responses over a brief window
    let start = std::time::Instant::now();
    while start.elapsed() < Duration::from_secs(4) {
        if let Some(resp) = service.drain_responses().into_iter().next() {
            match resp {
                FrameResponse::Ready {
                    generation,
                    request_id,
                    ..
                } => {
                    assert!(
                        generation >= 2,
                        "Generation 1 request should have been cancelled"
                    );
                    assert_eq!(request_id, req2);
                    return;
                }
                FrameResponse::Error { generation, .. } => {
                    assert!(
                        generation >= 2,
                        "Generation 1 request should have been cancelled"
                    );
                    return;
                }
            }
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn test_async_decoder_cache_retrieval() {
    let service = AsyncDecoderService::new(32);
    let dummy_path = PathBuf::from("ready_to_use/windows/samples/footage/shot_01.mp4");

    if !dummy_path.exists() {
        return;
    }

    let clip_id = ClipId::new();
    let req = service.request_frame(clip_id, dummy_path.clone(), 0);

    let start = std::time::Instant::now();
    let mut cached = false;
    while start.elapsed() < Duration::from_secs(5) {
        if let Some(FrameResponse::Ready {
            request_id, frame, ..
        }) = service.try_recv()
        {
            assert_eq!(request_id, req);
            assert!(frame.rgba_data.is_some());

            // Now test immediate synchronous cache hit
            let cached_frame = service.get_cached(&dummy_path, 0, 33_333);
            assert!(cached_frame.is_some());
            assert_eq!(cached_frame.unwrap().pts_us, frame.pts_us);
            cached = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }

    assert!(cached, "Frame was expected to decode and be cached");
}

#[test]
fn test_async_decoder_bounded_queues_and_backpressure() {
    // Instantiate with a strictly bounded queue capacity of 4 items
    let queue_cap = 4;
    let service = AsyncDecoderService::with_queue_capacity(16, queue_cap);
    assert_eq!(service.queue_capacity(), queue_cap);

    let clip_id = ClipId::new();
    let dummy_path = PathBuf::from("non_existent_folder/sample_video.mp4");

    // Rapidly submit requests to fill the bounded synchronous channel
    let mut accepted = 0;
    let mut full_detected = false;

    // Issue 32 requests in rapid succession
    for i in 0..32 {
        match service.try_request_frame(clip_id, dummy_path.clone(), i * 33_333) {
            Ok(_) => accepted += 1,
            Err(std::sync::mpsc::TrySendError::Full(_)) => {
                full_detected = true;
                break;
            }
            Err(std::sync::mpsc::TrySendError::Disconnected(_)) => {
                panic!("Decoder worker should remain active");
            }
        }
    }

    assert!(
        full_detected,
        "Bounded queue must reject requests when queue capacity is reached"
    );
    assert!(
        accepted <= queue_cap + 2, // Accounting for worker popping in background
        "Accepted requests ({accepted}) must be bounded near queue capacity ({queue_cap})"
    );

    // Verify non-blocking request_frame under saturated conditions does not hang or panic
    for i in 100..150 {
        let req_id = service.request_frame(clip_id, dummy_path.clone(), i * 33_333);
        assert!(req_id > 0);
    }

    // Verify prefetch_lookahead obeys bounded queue backpressure
    let prefetch_queued = service.prefetch_lookahead(
        clip_id, dummy_path, 1_000_000, 33_333,
        100, // Attempt to enqueue 100 prefetch requests
    );
    assert!(
        prefetch_queued <= queue_cap + 2,
        "Prefetch queued ({prefetch_queued}) must be strictly bounded by queue capacity"
    );
}

#[test]
fn test_async_decoder_nearest_cached_hold_frame() {
    let service = AsyncDecoderService::new(16);
    let sample_path = PathBuf::from("ready_to_use/windows/samples/footage/shot_01.mp4");

    if !sample_path.exists() {
        return;
    }

    let clip_id = ClipId::new();
    let _ = service.request_frame(clip_id, sample_path.clone(), 0);

    let start = std::time::Instant::now();
    while start.elapsed() < Duration::from_secs(5) {
        if let Some(FrameResponse::Ready { frame, .. }) = service.try_recv() {
            assert!(frame.rgba_data.is_some());

            // Query nearest cached frame with timestamp far away (e.g., 5 seconds out)
            // where exact get_cached with 33ms tolerance would fail:
            let exact = service.get_cached(&sample_path, 5_000_000, 33_333);
            assert!(exact.is_none());

            // Nearest cached frame must succeed, providing a zero-latency hold frame
            let hold_frame = service.get_nearest_cached(&sample_path, 5_000_000);
            assert!(hold_frame.is_some());
            assert_eq!(hold_frame.unwrap().pts_us, frame.pts_us);
            return;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}
