// Reddart iOS: Mobile Portrait Touch Video Editor
import SwiftUI

public struct IOSEditorView: View {
    @ObservedObject var bridge = ReddartBridgeIOS.shared
    @State private var showingExport = false
    @State private var showingLutPicker = false

    let lutPresets = [
        "Natural Rec.709",
        "Teal & Orange",
        "Warm Sunlight",
        "Vintage Film",
        "Black & White High Contrast"
    ]

    public var body: some View {
        VStack(spacing: 0) {
            // Top App Bar
            HStack {
                Text("Reddart")
                    .font(.title3)
                    .fontWeight(.heavy)
                    .foregroundColor(.white)

                Spacer()

                // Undo / Redo
                HStack(spacing: 16) {
                    Button(action: { bridge.undo() }) {
                        Image(systemName: "arrow.uturn.backward")
                            .font(.body)
                    }

                    Button(action: { bridge.redo() }) {
                        Image(systemName: "arrow.uturn.forward")
                            .font(.body)
                    }

                    Button(action: { showingExport = true }) {
                        HStack(spacing: 4) {
                            Image(systemName: "square.and.arrow.up")
                            Text("Export")
                                .fontWeight(.bold)
                        }
                        .font(.caption)
                        .padding(.horizontal, 12)
                        .padding(.vertical, 6)
                        .background(Color.cyan)
                        .foregroundColor(.black)
                        .cornerRadius(16)
                    }
                }
            }
            .padding(.horizontal, 16)
            .padding(.vertical, 10)
            .background(Color(UIColor.systemBackground))

            // Video Preview Viewport (9:16 vertical / 16:9 box)
            ZStack(alignment: .bottomTrailing) {
                IOSMetalPreview()
                    .frame(height: 320)
                    .background(Color.black)

                // Current timecode badge
                Text(bridge.formatTime(bridge.playheadUs))
                    .font(.system(.caption, design: .monospaced))
                    .fontWeight(.bold)
                    .padding(.horizontal, 8)
                    .padding(.vertical, 4)
                    .background(Color.black.opacity(0.7))
                    .foregroundColor(.white)
                    .cornerRadius(4)
                    .padding(8)
            }

            // Transport Playback Bar
            HStack(spacing: 30) {
                Button(action: { bridge.seek(to: 0) }) {
                    Image(systemName: "backward.end.fill")
                        .font(.title3)
                }

                Button(action: { bridge.togglePlayback() }) {
                    Image(systemName: bridge.isPlaying ? "pause.circle.fill" : "play.circle.fill")
                        .font(.system(size: 44))
                        .foregroundColor(.cyan)
                }

                Button(action: { bridge.seek(to: bridge.durationUs) }) {
                    Image(systemName: "forward.end.fill")
                        .font(.title3)
                }
            }
            .padding(.vertical, 10)

            Divider()

            // 120Hz Gesture Timeline
            IOSTimelineCanvas()
                .frame(height: 130)
                .background(Color(UIColor.secondarySystemBackground))

            Divider()

            // Mobile Quick Action Toolbar
            ScrollView(.horizontal, showsIndicators: false) {
                HStack(spacing: 16) {
                    ActionButton(icon: "scissors", title: "Split") {
                        bridge.splitClip()
                    }

                    ActionButton(icon: "trash", title: "Delete") {
                        bridge.deleteClip()
                    }

                    ActionButton(icon: "camera.filters", title: "3D LUT") {
                        showingLutPicker = true
                    }

                    ActionButton(icon: "captions.bubble", title: "AI Captions") {
                        bridge.generateAiCaptions()
                    }

                    ActionButton(icon: "waveform.badge.minus", title: "Denoise") {
                        bridge.denoiseAudio()
                    }

                    ActionButton(icon: "crop", title: "Reframe") {
                        bridge.autoReframe()
                    }
                }
                .padding(.horizontal, 16)
                .padding(.vertical, 12)
            }
            .background(Color(UIColor.systemBackground))

            // Status message footer
            HStack {
                Text(bridge.statusMessage)
                    .font(.caption2)
                    .foregroundColor(.secondary)
                Spacer()
                Text("Zero-Lag Core: Path C")
                    .font(.caption2)
                    .foregroundColor(.secondary)
            }
            .padding(.horizontal, 16)
            .padding(.bottom, 6)
        }
        .sheet(isPresented: $showingLutPicker) {
            NavigationView {
                List(lutPresets, id: \.self) { lut in
                    HStack {
                        Text(lut)
                        Spacer()
                        if bridge.activeLut == lut {
                            Image(systemName: "checkmark")
                                .foregroundColor(.cyan)
                        }
                    }
                    .contentShape(Rectangle())
                    .onTapGesture {
                        bridge.applyLut(lut)
                        showingLutPicker = false
                    }
                }
                .navigationTitle("Select 3D LUT")
                .navigationBarTitleDisplayMode(.inline)
            }
        }
        .sheet(isPresented: $showingExport) {
            VStack(spacing: 20) {
                Text("Export Video")
                    .font(.headline)
                    .padding(.top, 20)

                VStack(spacing: 12) {
                    ExportPresetButton(title: "TikTok / Reels (1080×1920 60fps)", icon: "bolt.fill") {
                        showingExport = false
                        bridge.startExport(preset: "TikTok Vertical")
                    }

                    ExportPresetButton(title: "YouTube Shorts (1080×1920)", icon: "play.rectangle.fill") {
                        showingExport = false
                        bridge.startExport(preset: "YouTube Shorts")
                    }

                    ExportPresetButton(title: "Instagram Post (1:1 Square)", icon: "square.fill") {
                        showingExport = false
                        bridge.startExport(preset: "Instagram Square")
                    }
                }
                .padding(.horizontal, 20)

                if bridge.isExporting {
                    ProgressView(value: bridge.exportProgress, total: 1.0)
                        .padding(.horizontal, 20)
                }

                Spacer()
            }
        }
    }
}

struct ActionButton: View {
    let icon: String
    let title: String
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            VStack(spacing: 4) {
                Image(systemName: icon)
                    .font(.body)
                Text(title)
                    .font(.caption2)
            }
            .frame(width: 56)
        }
        .foregroundColor(.primary)
    }
}

struct ExportPresetButton: View {
    let title: String
    let icon: String
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            HStack {
                Image(systemName: icon)
                    .foregroundColor(.cyan)
                Text(title)
                    .font(.subheadline)
                    .fontWeight(.medium)
                Spacer()
                Image(systemName: "chevron.right")
                    .font(.caption)
                    .foregroundColor(.secondary)
            }
            .padding()
            .background(Color(UIColor.secondarySystemBackground))
            .cornerRadius(10)
        }
        .foregroundColor(.primary)
    }
}

