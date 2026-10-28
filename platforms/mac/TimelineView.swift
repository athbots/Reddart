// Reddart macOS: 120Hz ProMotion Multi-Track Timeline Canvas
import SwiftUI

public struct TimelineView: View {
    @ObservedObject var bridge = ReddartBridge.shared
    @State private var zoomScale: CGFloat = 1.0

    let trackLabels = [
        ("V1", "Main Video", Color.blue),
        ("V2", "Overlays / Text", Color.purple),
        ("A1", "Master Audio", Color.green)
    ]

    public var body: some View {
        VStack(spacing: 0) {
            // Timeline Ruler & Control Bar
            HStack {
                HStack(spacing: 12) {
                    Button(action: { bridge.undo() }) {
                        Image(systemName: "arrow.uturn.backward")
                    }
                    .keyboardShortcut("z", modifiers: .command)
                    .help("Undo (Cmd+Z)")

                    Button(action: { bridge.redo() }) {
                        Image(systemName: "arrow.uturn.forward")
                    }
                    .keyboardShortcut("z", modifiers: [.command, .shift])
                    .help("Redo (Cmd+Shift+Z)")

                    Divider().frame(height: 16)

                    Button(action: { bridge.splitClip() }) {
                        Label("Split", systemImage: "scissors")
                    }
                    .keyboardShortcut("b", modifiers: .command)
                    .help("Split Clip at Playhead (Cmd+B)")

                    Button(action: { bridge.deleteSelectedClip() }) {
                        Label("Delete", systemImage: "trash")
                    }
                    .help("Delete Selected Clip")
                }
                .buttonStyle(.borderless)

                Spacer()

                // Timecode Display
                Text(bridge.formatTime(bridge.playheadUs))
                    .font(.system(.body, design: .monospaced))
                    .fontWeight(.bold)
                    .foregroundColor(.cyan)

                Spacer()

                // Zoom Slider
                HStack(spacing: 6) {
                    Image(systemName: "minus.magnifyingglass")
                    Slider(value: $zoomScale, in: 0.5...3.0)
                        .frame(width: 100)
                    Image(systemName: "plus.magnifyingglass")
                }
            }
            .padding(.horizontal, 16)
            .padding(.vertical, 8)
            .background(Color(NSColor.windowBackgroundColor))

            Divider()

            // Scrollable Tracks Canvas
            GeometryReader { geo in
                let totalWidth = max(geo.size.width * zoomScale, geo.size.width)

                ScrollView([.horizontal], showsIndicators: true) {
                    ZStack(alignment: .topLeading) {
                        // Background track lanes
                        VStack(spacing: 6) {
                            ForEach(0..<trackLabels.count, id: \.self) { idx in
                                let (code, name, color) = trackLabels[idx]
                                HStack(spacing: 8) {
                                    // Track header
                                    HStack {
                                        Text(code)
                                            .font(.caption2)
                                            .fontWeight(.bold)
                                            .padding(.horizontal, 4)
                                            .padding(.vertical, 2)
                                            .background(color.opacity(0.3))
                                            .cornerRadius(3)
                                        Text(name)
                                            .font(.caption)
                                            .foregroundColor(.secondary)
                                        Spacer()
                                        Image(systemName: "speaker.wave.2")
                                            .font(.caption2)
                                            .foregroundColor(.secondary)
                                    }
                                    .frame(width: 140)
                                    .padding(.leading, 8)

                                    // Track Lane with clips
                                    ZStack(alignment: .leading) {
                                        RoundedRectangle(cornerRadius: 4)
                                            .fill(Color.black.opacity(0.3))
                                            .frame(height: 48)

                                        // Render Clip Blocks
                                        if idx == 0 {
                                            // Video Track 1: Two clips
                                            HStack(spacing: 2) {
                                                ClipBlockView(title: "Clip_01.mp4", duration: "4.0s", color: .blue)
                                                    .frame(width: totalWidth * 0.45)
                                                ClipBlockView(title: "Clip_02.mp4", duration: "4.0s", color: .indigo)
                                                    .frame(width: totalWidth * 0.45)
                                            }
                                        } else if idx == 1 {
                                            // Text/Overlay Track
                                            HStack(spacing: 4) {
                                                Spacer().frame(width: totalWidth * 0.1)
                                                ClipBlockView(title: "AI Subtitles [Whisper]", duration: "2.3s", color: .purple)
                                                    .frame(width: totalWidth * 0.35)
                                            }
                                        } else {
                                            // Audio Track 1
                                            ClipBlockView(title: "ambient.wav [48kHz]", duration: "8.0s", color: .green)
                                                .frame(width: totalWidth * 0.9)
                                        }
                                    }
                                    .frame(width: totalWidth)
                                }
                            }
                        }
                        .padding(.vertical, 8)

                        // Red Playhead Indicator Line
                        let progress = Double(bridge.playheadUs) / Double(bridge.durationUs)
                        let playheadX = 148 + CGFloat(progress) * totalWidth

                        Rectangle()
                            .fill(Color.red)
                            .frame(width: 2, height: CGFloat(trackLabels.count * 54 + 20))
                            .offset(x: playheadX)
                            .overlay(
                                TriangleIndicator()
                                    .fill(Color.red)
                                    .frame(width: 12, height: 10)
                                    .offset(x: playheadX, y: -5),
                                alignment: .topLeading
                            )
                    }
                    .frame(minWidth: totalWidth + 180)
                    .contentShape(Rectangle())
                    .gesture(
                        DragGesture(minimumDistance: 0)
                            .onChanged { value in
                                let clickX = value.location.x - 148
                                if clickX >= 0 {
                                    let fraction = Double(clickX / totalWidth).clamped(to: 0.0...1.0)
                                    let targetUs = Int64(fraction * Double(bridge.durationUs))
                                    bridge.seek(to: targetUs)
                                }
                            }
                    )
                }
            }
            .background(Color(NSColor.controlBackgroundColor))
        }
    }
}

struct ClipBlockView: View {
    let title: String
    let duration: String
    let color: Color

    var body: some View {
        HStack {
            VStack(alignment: .leading, spacing: 2) {
                Text(title)
                    .font(.caption)
                    .fontWeight(.medium)
                    .lineLimit(1)
                Text(duration)
                    .font(.caption2)
                    .foregroundColor(.white.opacity(0.8))
            }
            Spacer()
        }
        .padding(.horizontal, 8)
        .frame(height: 44)
        .background(
            LinearGradient(colors: [color.opacity(0.85), color.opacity(0.65)], startPoint: .top, endPoint: .bottom)
        )
        .cornerRadius(6)
        .overlay(
            RoundedRectangle(cornerRadius: 6)
                .stroke(Color.white.opacity(0.3), lineWidth: 1)
        )
    }
}

struct TriangleIndicator: Shape {
    func path(in rect: CGRect) -> Path {
        var path = Path()
        path.move(to: CGPoint(x: rect.midX, y: rect.maxY))
        path.addLine(to: CGPoint(x: rect.minX, y: rect.minY))
        path.addLine(to: CGPoint(x: rect.maxX, y: rect.minY))
        path.closeSubpath()
        return path
    }
}

extension Comparable {
    func clamped(to limits: ClosedRange<Self>) -> Self {
        return min(max(self, limits.lowerBound), limits.upperBound)
    }
}
