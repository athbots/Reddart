# Creator Workflows, Onboarding Architecture & Closed-Alpha Plan

**Document Reference:** RD-ALPHA-2026-V1  
**Target:** Reddart Video Editor v0.2.1+  
**Focus:** Creator Transition Ergonomics, 60-Second First Export, and Alpha Program Execution  

---

## 1. Creator Workflow Analysis & Switching Barrier Audit

To achieve rapid organic adoption, Reddart specifically addresses the friction points and cognitive hurdles creators encounter when migrating from legacy tools.

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                    Creator Cohort Transition Matrix                         │
├─────────────────────┬───────────────────────────────┬───────────────────────┤
│ Creator Persona     │ Primary Legacy Platform       │ Core Friction & Fix   │
├─────────────────────┼───────────────────────────────┼───────────────────────┤
│ Short-Form Viral    │ CapCut / Premiere Rush        │ Watermarks & Clouds   │
│ (TikTok / Reels)    │                               │ ➜ Built-in 9:16 Deck  │
├─────────────────────┼───────────────────────────────┼───────────────────────┤
│ Long-Form Narrative │ Adobe Premiere Pro            │ $600/yr Rental & Lag  │
│ (YouTube / Vlog)    │                               │ ➜ Native NLE Shortcuts│
├─────────────────────┼───────────────────────────────┼───────────────────────┤
│ Production Studio   │ DaVinci Resolve Studio        │ Heavy Hardware Floor  │
│ (Documentary / Ads) │                               │ ➜ 10MB Standalone App │
└─────────────────────┴───────────────────────────────┴───────────────────────┘
```

### 1.1 Short-Form Viral Creators (TikTok, Instagram Reels, YouTube Shorts)
- **Observed Workflow:** High-volume, rapid turnaround. Ingests vertical 9:16 phone clips, applies beat cuts, adds captions, and exports immediately.
- **Legacy Switching Barriers:**
  - CapCut forces cloud accounts, injects trailing watermarks, and throttles 4K/60fps exports behind monthly subscription paywalls.
  - Premiere Pro requires complex sequence setup dialogs and custom sequence aspect ratio adjustments.
- **Reddart Ergonomic Solution:**
  - Dedicated **Starter Project Templates (`Ctrl+T`)**: One-click instantiation of "TikTok / Reels Beat Cut" (1080x1920, 60 fps, 10.0s sequence with 5 pre-configured media slots and transient cut cues).
  - Native 9:16 pillarboxed viewport rendering with zero distortion.
  - Continuous variable speed remapping ($0.25\times\text{--}4.0\times$) for velocity ramps.

### 1.2 Long-Form Narrative Creators (YouTube Tech, Documentary, Video Essays)
- **Observed Workflow:** Multi-track editing with extensive A-roll talking head and B-roll cutaways. Relies heavily on keyboard muscle memory for continuous timeline scrubbing and cutting.
- **Legacy Switching Barriers:**
  - Editing speed drops when standard keyboard shortcuts differ from industry standards.
  - Project file corruption and UI stuttering during long timelines (1,000+ cuts).
- **Reddart Ergonomic Solution:**
  - 100% Industry-standard NLE shortcuts: Razor Split (`C`), Ripple Trim Out (`B`), Roll Edit (`N`), Slip (`Y`), Slide (`U`), Shuttle (`J`/`K`/`L`), and Mark In/Out (`I`/`O`).
  - Append-only Write-Ahead Log (`.reddart.wal`) with CRC32 recovery, guaranteeing zero edit loss on unexpected shutdown.
  - Microsecond DAG compilation latency ($< 50\text{ µs}$ for 800 clips across 16 tracks), eliminating scrub stutter.

### 1.3 Professional Studios & Privacy-Conscious Creators
- **Observed Workflow:** Client confidentiality, local network storage, 3D LUT grading, and calibrated master exports.
- **Legacy Switching Barriers:**
  - Unwanted telemetry, cloud synchronization, and software that requires internet connectivity for license checks.
- **Reddart Ergonomic Solution:**
  - 100% offline, standalone architecture with zero network sockets opened during runtime.
  - Standard `.cube` 3D LUT import with hardware-accelerated tetrahedral interpolation.
  - Clean dual licensing (MIT OR Apache-2.0).

---

## 2. Professional Onboarding: The 60-Second First-Export Journey

A key goal for Reddart v0.2.1 is enabling first-time users to achieve a successful video export within 60 seconds of unzipping the binary, without consulting technical documentation.

```
00:00                00:15                00:30                00:45                00:60
  │                    │                    │                    │                    │
Launch Reddart.exe  Preview Playback     Make Split Cut       Open Export Modal    MP4 Complete
(Zero Installer)    (Spacebar / WASAPI)  (Press 'C' Razor)    (Press 'Ctrl+E')     (Hardware Fast)
```

### Step-by-Step User Journey:
1. **Zero-Friction Launch (00:00 – 00:10):**  
   User double-clicks `Reddart.exe`. The application opens immediately with Direct3D 12 acceleration. No installation wizard, no user registration, and no cloud logins.
2. **Instant Visual Feedback (00:10 – 00:25):**  
   The status bar guides the user immediately:  
   `"Reddart Studio v0.2.1 | Press Space to preview, Ctrl+T for templates, Ctrl+E to export"`  
   If launched with sample media, the timeline is pre-populated with a demo clip. Pressing `Space` initiates smooth 60 fps playback with real-time 48 kHz stereo audio and peak dBFS metering.
3. **Interactive Timeline Editing (00:25 – 00:40):**  
   User scrubs the playhead across the timeline ruler. Pressing `C` slices the clip at the playhead. Pressing `B` ripple-trims the cut point.
4. **Export in Two Clicks (00:40 – 00:60):**  
   User presses `Ctrl+E` (or clicks "🚀 Export..."). The hardware export modal appears with the optimal platform preset (`youtube_1080p`) pre-selected. Clicking "Render" engages native GPU NVENC/QuickSync encoding, producing a finished MP4 file in under 5 seconds.

---

## 3. Closed-Alpha Recruitment & Testing Framework

### 3.1 Target Cohort Profile
The initial closed-alpha phase targets **25 diverse content creators**:
- **10 YouTube Creators:** Running Windows 10/11 desktops, producing weekly 10–25 minute widescreen videos.
- **10 Short-Form Creators:** Producing daily 9:16 vertical TikTok/Reels content using phone footage (iPhone 4K 60fps HEVC, Samsung MP4).
- **5 Audio-Centric Creators:** Podcasters and musicians evaluating multi-track stereo mixing, 3-band EQ, and master bus limiter headroom.

### 3.2 Structured Alpha Test Script

| Phase | Milestone Task | Verification Metric |
|---|---|---|
| **Test 1** | Clean Unzip & Immediate Launch | Executable launches without SmartScreen blocking or missing DLL warnings. |
| **Test 2** | Media Ingestion | Drag and drop 5 mixed format files (H.264, HEVC, WAV) into the Media Bin. |
| **Test 3** | Timeline Manipulation | Execute 10 cuts using Razor (`C`) and Ripple Trim (`B`). |
| **Test 4** | Effects & Automation | Apply a 3D LUT and keyframe an opacity fade-out using `EaseInOut`. |
| **Test 5** | Crash Recovery Simulation | Force-kill `Reddart.exe` via Task Manager; relaunch and verify WAL recovery. |
| **Test 6** | Hardware Master Export | Export sequence using `youtube_1080p` and `vertical_9_16_1080p`. |

### 3.3 Telemetry-Free Feedback Intake Protocol
In accordance with Reddart's strict privacy policy, no telemetry is gathered automatically. Feedback is gathered via:
1. **Local Diagnostic Log:** When a crash occurs, users can view local logs in `appdata/Reddart/logs/` and manually submit them.
2. **Structured GitHub Alpha Issue Form:** Pre-configured with environment tags (GPU model, driver version, Windows build).
3. **Weekly Asynchronous Creator Check-In:** Focuses on timeline responsiveness, missing audio features, and UI scaling feedback.

