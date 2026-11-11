# Reddart Creator Onboarding Guide: 60 Seconds to First Export

**Target Audience:** Video Creators, Editors, Motion Designers  
**Application:** Reddart Desktop Studio (Windows 10/11)  
**Time Required:** 1 Minute  
**Prerequisites:** None (Standalone portable executable)  

---

## Welcome to Reddart

Reddart is designed to be fast, responsive, and completely private. It does not require creating an account, entering credit card information, or waiting through software installers. 

Follow this quick guide to launch the studio, perform basic edits, and render your first high-definition video in under 60 seconds.

---

## The 60-Second Walkthrough

```
┌──────────────────┐     ┌──────────────────┐     ┌──────────────────┐     ┌──────────────────┐
│  Step 1: Launch  │ ──▶ │ Step 2: Preview  │ ──▶ │  Step 3: Edit    │ ──▶ │  Step 4: Export  │
│  (Zero Install)  │     │   (Spacebar)     │     │  (Press 'C' Cut) │     │  (Press Ctrl+E)  │
└──────────────────┘     └──────────────────┘     └──────────────────┘     └──────────────────┘
```

### Step 1: Launch Reddart (0 – 10 Seconds)
1. Download `Reddart-v0.2.1-Windows-x86_64.zip` and extract the folder.
2. Double-click `Reddart.exe`.
3. The studio interface appears immediately with hardware-accelerated rendering.

---

### Step 2: Instant Playback (10 – 20 Seconds)
1. If launched with default sample files, a demo sequence will already be loaded on the timeline.
2. Press `Space` to start playback.
3. Observe real-time 60 fps preview in the central monitor, along with the 48 kHz stereo VU meter in the right panel. Press `Space` again to pause.

---

### Step 3: Trim and Edit (20 – 40 Seconds)
Reddart uses standard editing shortcuts found across industry NLEs:

1. **Scrubbing:** Drag the red playhead line on the timeline ruler to the position you want to cut.
2. **Razor Split (`C`):** Hover over a clip and press `C` to slice it into two independent clips at the playhead.
3. **Ripple Trim (`B`):** Press `B` to select the ripple trim tool. Drag the head or tail of a clip to shorten it; all downstream clips automatically slide left to eliminate gaps.
4. **Shuttle Controls:**
   - Press `L` to shuttle forward (press multiple times for 2x, 4x, 8x).
   - Press `K` to stop.
   - Press `J` to shuttle backwards.

---

### Step 4: Export Your Finished Video (40 – 60 Seconds)
1. Press `Ctrl + E` (or click **🚀 Export...** on the top toolbar).
2. The Hardware Export dialog opens with the `YouTube 1080p` preset pre-selected.
3. Select an output location and click **Start Export**.
4. Reddart automatically leverages your GPU hardware encoder (NVIDIA NVENC, AMD AMF, or Intel QuickSync) to produce the MP4 master at up to 2x real-time speed.

---

## Essential Keyboard Shortcuts Cheat Sheet

| Key | Function | Description |
|---|---|---|
| `Space` | Play / Pause | Toggle real-time sequence playback |
| `J` / `K` / `L` | Shuttle Controls | Reverse, Stop, Forward (1x, 2x, 4x, 8x) |
| `V` | Selection Tool | Select, reposition, or drag clip handles |
| `C` | Razor Cut | Split active clip at playhead |
| `B` | Ripple Trim | Trim clip boundary and collapse empty gap |
| `N` | Roll Edit | Move boundary between clips preserving overall length |
| `Y` | Slip Tool | Shift media source window inside fixed timeline duration |
| `U` | Slide Tool | Move clip position while adjusting neighboring clip boundaries |
| `I` / `O` | Mark In / Out | Set sequence in and out points for partial export |
| `M` | Add Marker | Add timestamp cue marker at playhead |
| `Ctrl + T` | Templates | Open starter template chooser (TikTok, Vlog, 4K) |
| `Ctrl + S` | Save | Commit write-ahead log checkpoint |
| `Ctrl + E` | Export | Open hardware export dialog |
| `Ctrl + Z` | Undo | Revert last timeline modification |
| `Ctrl + Y` | Redo | Re-apply last undone modification |

---

## Getting Help & Community

- **Starter Templates:** Press `Ctrl + T` to instantiate pre-built projects configured for TikTok/Reels (9:16 vertical), YouTube (16:9 4K/1080p), or Square (1:1).
- **Issue Tracker & Discussions:** [https://github.com/athbots/Reddart/issues](https://github.com/athbots/Reddart/issues)
- **Website & Documentation:** [https://athbots.github.io/Reddart/](https://athbots.github.io/Reddart/)

