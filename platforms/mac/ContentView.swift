// Reddart macOS: Professional 3-Column Studio Interface
import SwiftUI

public struct ContentView: View {
    @ObservedObject var bridge = ReddartBridge.shared
    @State private var selectedTab = 0
    @State private var showingExportModal = false
    @State private var selectedExportPreset = "Apple ProRes 422"

    let lutPresets = [
        "Natural Rec.709",
        "Teal & Orange",
        "Warm Sunlight",
        "Vintage Film",
        "Black & White High Contrast"
    ]

    public var body: some View {
        HSplitView {
            // Left Column: Asset & Effect Library
            VStack(spacing: 0) {
                Picker("", selection: $selectedTab) {
                    Text("Media").tag(0)
                    Text("3D LUTs").tag(1)
                    Text("AI Tools").tag(2)
                }
                .pickerStyle(.segmented)
                .padding(10)

                Divider()

                if selectedTab == 0 {
                    // Media Browser
                    List {
                        Section("Project Assets") {
                            Label("clip_01.mp4 (4K 60fps)", systemImage: "video.fill")
                            Label("clip_02.mp4 (1080p 60fps)", systemImage: "video.fill")
                            Label("ambient.wav (Stereo 48kHz)", systemImage: "waveform")
                        }
                        Section("Starter Templates") {
                            Label("TikTok / Reels Beat Cut", systemImage: "bolt.fill")
                            Label("Cinematic Vlog (24fps)", systemImage: "film.fill")
                            Label("Social Square (1:1)", systemImage: "square.fill")
                        }
                    }
                } else if selectedTab == 1 {
                    // 3D LUT Presets
                    List(lutPresets, id: \.self) { lut in
                        HStack {
                            Text(lut)
                                .font(.body)
                            Spacer()
                            if bridge.activeLut == lut {
                                Image(systemName: "checkmark.circle.fill")
                                    .foregroundColor(.cyan)
                            }
                        }
                        .contentShape(Rectangle())
                        .onTapGesture {
                            bridge.applyLut(lut)
                        }
                    }
                } else {
                    // AI Neural Engine Tools
                    VStack(alignment: .leading, spacing: 14) {
                        Button(action: { bridge.generateAiCaptions() }) {
                            HStack {
                                Image(systemName: "captions.bubble.fill")
                                    .foregroundColor(.purple)
                                VStack(alignment: .leading) {
                                    Text("Auto Captions")
                                        .fontWeight(.semibold)
                                    Text("Whisper quantized model")
                                        .font(.caption)
                                        .foregroundColor(.secondary)
                                }
                            }
                        }
                        .buttonStyle(.plain)

                        Divider()

                        Button(action: { bridge.denoiseAudio() }) {
                            HStack {
                                Image(systemName: "waveform.path.badge.minus")
                                    .foregroundColor(.green)
                                VStack(alignment: .leading) {
                                    Text("Denoise Audio")
                                        .fontWeight(.semibold)
                                    Text("Spectral Gating (-15 dB)")
                                        .font(.caption)
                                        .foregroundColor(.secondary)
                                }
                            }
                        }
                        .buttonStyle(.plain)

                        Divider()

                        Button(action: { bridge.autoReframe(aspectW: 9, aspectH: 16) }) {
                            HStack {
                                Image(systemName: "crop")
                                    .foregroundColor(.orange)
                                VStack(alignment: .leading) {
                                    Text("Auto Reframe to 9:16")
                                        .fontWeight(.semibold)
                                    Text("Smooth trajectory solver")
                                        .font(.caption)
                                        .foregroundColor(.secondary)
                                }
                            }
                        }
                        .buttonStyle(.plain)

                        Spacer()
                    }
                    .padding(14)
                }
            }
            .frame(minWidth: 220, idealWidth: 260, maxWidth: 320)

            // Center Column: Metal High-Res Viewport + Timeline
            VSplitView {
                // Viewport & Transport
                VStack(spacing: 0) {
                    // 120Hz ProMotion Metal Viewport
                    MetalPreviewView()
                        .frame(minHeight: 280)
                        .background(Color.black)

                    // Transport Controls
                    HStack(spacing: 20) {
                        Button(action: { bridge.seek(to: 0) }) {
                            Image(systemName: "backward.end.fill")
                        }
                        .help("Go to Start")

                        Button(action: { bridge.seek(to: bridge.playheadUs - 1_000_000) }) {
                            Image(systemName: "gobackward.10")
                        }
                        .help("Back 1s")

                        Button(action: { bridge.togglePlayback() }) {
                            Image(systemName: bridge.isPlaying ? "pause.fill" : "play.fill")
                                .font(.title2)
                        }
                        .keyboardShortcut(.space, modifiers: [])
                        .help("Play / Pause (Space)")

                        Button(action: { bridge.seek(to: bridge.playheadUs + 1_000_000) }) {
                            Image(systemName: "goforward.10")
                        }
                        .help("Forward 1s")

                        Spacer()

                        // Active LUT & Quality Tag
                        Text(bridge.activeLut)
                            .font(.caption)
                            .padding(.horizontal, 8)
                            .padding(.vertical, 3)
                            .background(Color.cyan.opacity(0.2))
                            .foregroundColor(.cyan)
                            .cornerRadius(4)

                        // Status message
                        Text(bridge.statusMessage)
                            .font(.caption)
                            .foregroundColor(.secondary)
                    }
                    .padding(.horizontal, 16)
                    .padding(.vertical, 8)
                    .background(Color(NSColor.windowBackgroundColor))
                }
                .frame(minHeight: 320)

                // Bottom: 120Hz Multi-Track Timeline
                TimelineView()
                    .frame(minHeight: 200, idealHeight: 280)
            }

            // Right Column: Inspector Panel
            VStack(alignment: .leading, spacing: 14) {
                Text("Inspector")
                    .font(.headline)
                    .padding(.top, 12)

                Divider()

                Group {
                    Text("Project Info")
                        .font(.subheadline)
                        .fontWeight(.semibold)

                    LabeledContent("Resolution", value: "1920 × 1080 (16:9)")
                    LabeledContent("Frame Rate", value: "60.00 fps (ProMotion)")
                    LabeledContent("Engine Core", value: "Shared Rust Path C")
                    LabeledContent("Acceleration", value: "Metal + VideoToolbox")
                }
                .font(.caption)

                Divider()

                // Export Button
                Button(action: { showingExportModal = true }) {
                    HStack {
                        Spacer()
                        Image(systemName: "square.and.arrow.up")
                        Text("Export Video...")
                            .fontWeight(.bold)
                        Spacer()
                    }
                    .padding(.vertical, 8)
                }
                .buttonStyle(.borderedProminent)
                .tint(.cyan)
                .keyboardShortcut("e", modifiers: .command)

                if bridge.isExporting {
                    VStack(alignment: .leading, spacing: 4) {
                        ProgressView(value: bridge.exportProgress, total: 1.0)
                        Text(String(format: "Rendering: %.0f%%", bridge.exportProgress * 100))
                            .font(.caption2)
                            .foregroundColor(.secondary)
                    }
                }

                Spacer()
            }
            .padding(.horizontal, 14)
            .frame(minWidth: 200, idealWidth: 240, maxWidth: 300)
        }
        .frame(minWidth: 960, minHeight: 640)
        .sheet(isPresented: $showingExportModal) {
            VStack(spacing: 16) {
                Text("Export Project")
                    .font(.title2)
                    .fontWeight(.bold)

                Picker("Preset", selection: $selectedExportPreset) {
                    Text("Apple ProRes 422 (Master)").tag("Apple ProRes 422")
                    Text("HEVC / H.265 (High Quality)").tag("HEVC / H.265")
                    Text("YouTube 4K UHD 60fps").tag("YouTube 4K")
                    Text("TikTok / Reels Vertical 1080p").tag("TikTok Vertical")
                }
                .pickerStyle(.radioGroup)

                HStack {
                    Button("Cancel") {
                        showingExportModal = false
                    }
                    Button("Start Hardware Export") {
                        showingExportModal = false
                        bridge.startExport(preset: selectedExportPreset)
                    }
                    .buttonStyle(.borderedProminent)
                }
            }
            .padding(24)
            .frame(width: 400)
        }
    }
}
