# Reddart Feature Implementation & Verification Matrix (v0.2.1 / v1.0 Roadmap)

## Classification Schema

Each advertised product feature is audited against actual working code, user-visible UI entry points, automated regression test coverage, and remaining technical limitations:

1. **Implemented and Verified in Desktop Workflow**: Fully operational in GUI studio or CLI, covered by automated integration tests, and manually verified.
2. **Partially Implemented**: Core engine logic exists and functions in export or backend pipelines, but UI surface or interactive tooling is currently incomplete.
3. **Experimental**: Underlying algorithms or crate implementations exist, but are intentionally gated, deactivated in production desktop workflows, or deferred.
4. **Not Implemented**: Planned roadmap item not yet present in the codebase.

---

## Comprehensive Feature Matrix

| # | Feature / Capability | Classification | Source Files | UI Entry Point | Automated Test | Remaining Limitations |
|---|---|---|---|---|---|---|
| 1 | **Multi-Track Timeline Deck** | **1. Implemented & Verified** | `crates/rd-core/src/timeline.rs`<br>`platforms/windows/src/app.rs` | Main Timeline panel, Track headers (Mute/Solo/Lock) | `tests/timeline_correctness.rs`<br>`crates/rd-core/src/timeline.rs` | Uniform track heights; track grouping folders deferred post-v1. |
| 2 | **Selection & Drag Repositioning (`V`)** | **1. Implemented & Verified** | `crates/rd-core/src/track.rs`<br>`platforms/windows/src/app.rs` | Toolbar `Selection (V)`, Click & Drag clips | `tests/timeline_correctness.rs`<br>`crates/rd-core/tests/editing_operations_matrix.rs` | Snapping adheres to magnetic boundaries. |
| 3 | **Razor Split Tool (`C`, `S`)** | **1. Implemented & Verified** | `crates/rd-core/src/track.rs`<br>`platforms/windows/src/app.rs` | Toolbar `Razor (C)`, Menu `Edit -> Split Clip (S)` | `crates/rd-core/src/track.rs::test_split_clip`<br>`tests/timeline_correctness.rs` | Splits selected clip or active track clip at playhead. |
| 4 | **Ripple Trim Tool (`B`)** | **1. Implemented & Verified** | `crates/rd-core/src/track.rs`<br>`platforms/windows/src/app.rs` | Toolbar `Ripple Trim (B)`, edge drag handles | `tests/editing_operations_matrix.rs::test_ripple_trim_in_positive_and_negative_matrix` | Shifts subsequent clips on the active track; other tracks require track lock. |
| 5 | **Roll Edit Tool (`N`)** | **1. Implemented & Verified** | `crates/rd-core/src/track.rs`<br>`platforms/windows/src/app.rs` | Toolbar `Roll (N)`, cut boundary dragging | `crates/rd-core/src/track.rs::test_roll_edit_invariants` | Clamped when either adjacent clip hits media source boundary. |
| 6 | **Slip Tool (`Y`)** | **1. Implemented & Verified** | `crates/rd-core/src/track.rs`<br>`platforms/windows/src/app.rs` | Toolbar `Slip (Y)`, clip interior dragging | `crates/rd-core/src/track.rs::test_slip_edit_invariants` | Shifts source in/out window preserving sequence timing. |
| 7 | **Slide Tool (`U`)** | **1. Implemented & Verified** | `crates/rd-core/src/track.rs`<br>`platforms/windows/src/app.rs` | Toolbar `Slide (U)` | `tests/editing_operations_matrix.rs::test_slide_edit_comprehensive_matrix` | Requires predecessor and successor clips on the track. |
| 8 | **Ripple Delete (`Shift+Del`)** | **1. Implemented & Verified** | `crates/rd-core/src/track.rs`<br>`platforms/windows/src/app.rs` | Shortcut `Shift+Delete`, Context menu | `tests/timeline_correctness.rs::test_ripple_delete_compound_operation` | Removes clip and closes gap automatically. |
| 9 | **Markers & Work Areas (`M`, `I`, `O`)** | **1. Implemented & Verified** | `crates/rd-core/src/timeline.rs`<br>`platforms/windows/src/app.rs` | Ruler buttons, Shortcuts `M` (Marker), `I` (In), `O` (Out) | `crates/rd-core/src/timeline.rs::test_snap_points_and_snap_to` | Timeline-global markers; clip-attached markers deferred. |
| 10 | **Keyframe Automation & Splines** | **1. Implemented & Verified** | `crates/rd-core/src/keyframe.rs`<br>`crates/rd-core/src/clip.rs`<br>`platforms/windows/src/app.rs` | Clip Inspector Keyframe toggles & curve dropdowns | `crates/rd-core/src/clip.rs::test_clip_keyframe_animation`<br>`crates/rd-export/tests/media_corpus_test.rs` | Linear, EaseIn, EaseOut, EaseInOut, Hold supported; graphical curve editor canvas is post-v1. |
| 11 | **Geometric Shape Masking** | **1. Implemented & Verified** | `crates/rd-effects/src/mask.rs`<br>`crates/rd-render/src/graph.rs` | Clip Inspector Mask Panel (Rectangle, Ellipse, Gradient, Vignette) | `crates/rd-effects/src/mask.rs` unit tests<br>`crates/rd-export/tests/media_corpus_test.rs` | Parametric shapes with feather and invert; freehand polygon bezier pen rotoscoping is post-v1. |
| 12 | **Color Grading & 3D LUT Engine** | **1. Implemented & Verified** | `crates/rd-effects/src/color.rs`<br>`crates/rd-effects/src/lut.rs`<br>`platforms/windows/src/app.rs` | Color Grade Panel (Exposure, Contrast, Saturation, Temp, Tint, LUT Loader) | `crates/rd-effects/src/lut.rs` unit tests<br>`crates/rd-export/tests/media_corpus_test.rs` | Supports industry-standard .cube files up to 64x64x64. |
| 13 | **WASAPI Real-time Audio Engine (48 kHz)** | **1. Implemented & Verified** | `crates/rd-audio/src/engine.rs`<br>`mixer.rs`<br>`limiter.rs`<br>`platforms/windows/src/app.rs` | Real-time audio playback, Audio Mixer panel, VU peak meter with dBFS | `crates/rd-audio/src/lib.rs` tests<br>`crates/rd-export/tests/sample_project_render_test.rs` | Native 48 kHz 32-bit float stereo pipeline with tanh soft limiter; VST3 plugin hosting is post-v1. |
| 14 | **Variable Playback Speed & Shuttle** | **1. Implemented & Verified** | `crates/rd-core/src/clip.rs`<br>`crates/rd-audio/src/mixer.rs`<br>`platforms/windows/src/app.rs` | Clip Inspector Speed slider (0.25x - 4.0x), J-K-L Shuttle keys | `crates/rd-core/src/clip.rs::test_clip_duration_double_speed` | Real-time audio resampling; optical flow motion estimation is post-v1. |
| 15 | **Write-Ahead Log (WAL) Journal** | **1. Implemented & Verified** | `crates/rd-core/src/journal.rs`<br>`crates/rd-core/src/command.rs`<br>`platforms/windows/src/app.rs` | Automatic background durability, recovery prompt on startup | `crates/rd-core/tests/wal_fault_injection_test.rs` (6 fault tests) | CRC32 verified, torn-write truncation, sequence continuity enforced. Single active process per project. |
| 16 | **Missing Media Detection & Relinking** | **1. Implemented & Verified** | `crates/rd-core/src/project.rs`<br>`crates/rd-core/src/command.rs`<br>`platforms/windows/src/app.rs` | Program Monitor Offline Banner, itemized Relink dialog, Timeline hazard stripes | `crates/rd-core/tests/missing_media_test.rs` (3 tests) | Explicit offline graphic; original reference preserved until confirmed; no silent synthetic replacement. |
| 17 | **Hardware-Accelerated Export** | **1. Implemented & Verified** | `crates/rd-export/src/pipeline.rs`<br>`hardware.rs`<br>`platforms/windows/src/main.rs` | Export Modal (`Ctrl+E`), CLI `reddart export` | `crates/rd-export/tests/release_suite.rs`<br>`crates/rd-export/tests/sample_project_render_test.rs` | Automatically probes NVENC, AMF, QSV; fallback to bundled libx264. |
| 18 | **Starter Project Templates** | **1. Implemented & Verified** | `crates/rd-effects/src/templates.rs`<br>`platforms/windows/src/app.rs` | Menu `File -> New from Template... (Ctrl+T)`, CLI `reddart templates` | `crates/rd-export/tests/sample_project_render_test.rs::test_render_all_five_starter_templates` | 5 pre-calibrated multi-format templates (Reels, Vlog, Square, 4K Doc, Action). User custom templates post-v1. |
| 19 | **Headless Terminal CLI** | **1. Implemented & Verified** | `platforms/windows/src/main.rs` | Terminal commands: `reddart export`, `probe`, `templates`, `benchmark` | Automated in packaging validation & CLI tests | Headless rendering with ASCII progress bar and speed readout. |
| 20 | **Text & Subtitle Generator** | **2. Partially Implemented** | `crates/rd-core/src/clip.rs`<br>`crates/rd-export/tests/media_corpus_test.rs` | Media Bin text clip representation | `crates/rd-export/tests/media_corpus_test.rs::test_corpus_render_with_title_overlay` | Renders titles and caption overlays via export pipeline; interactive in-monitor WYSIWYG text formatting editor is under active development for v1.1. Workaround: import transparent PNG titles. |
| 21 | **On-Device Neural ML (Scene Detect / STT)** | **3. Experimental** | `crates/rd-ml/src/lib.rs` | None in desktop UI (intentionally gated) | `crates/rd-ml` unit tests | Neural ML capabilities deferred post-v1 per executive roadmap directive. |
| 22 | **Nested Compound Clips / Sub-Sequences** | **4. Not Implemented** | N/A | None | N/A | Flattened multi-track sequences only in v1.0. Scheduled for Phase II. |
| 23 | **Multi-Channel Surround Audio (5.1 / 7.1)** | **4. Not Implemented** | N/A | None | N/A | Studio stereo (2.0) bus only. Multi-channel surround slated for Phase II. |
| 24 | **macOS / Linux GUI Desktop App** | **2. Partially Implemented** | `crates/rd-apple`<br>`crates/rd-android` | CLI / core crates compile | Cross-crate unit tests | Pre-built GUI binary distribution is currently Windows-first (`Reddart.exe`). |

---

## Public Documentation Alignment

Public-facing documentation (`README.md`, `ready_to_use/windows/manifest.json`, and `docs/RELEASE_NOTES_AND_LIMITATIONS.md`) has been synchronized:
1. **Neural ML Deferred**: Marked as post-v1 experimental.
2. **Title Generator Status**: Documented as export/overlay-rendered, noting that rich in-monitor WYSIWYG typography is targeted for v1.1.
3. **Surround & Nested Clips**: Clearly documented as Phase II roadmap deliverables with documented workarounds.
