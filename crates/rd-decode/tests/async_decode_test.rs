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
