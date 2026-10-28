// Reddart iOS: Touch Gesture 120Hz ProMotion Timeline Canvas
import SwiftUI

public struct IOSTimelineCanvas: View {
    @ObservedObject var bridge = ReddartBridgeIOS.shared
    @State private var currentZoom: CGFloat = 1.0

    public var body: some View {
        GeometryReader { geo in
            let totalWidth = geo.size.width * currentZoom

            ZStack(alignment: .topLeading) {
                // Background Track Lanes
                VStack(spacing: 4) {
                    // Video Track 1
                    HStack(spacing: 2) {
                        RoundedRectangle(cornerRadius: 6)
                            .fill(LinearGradient(colors: [.blue, .blue.opacity(0.7)], startPoint: .top, endPoint: .bottom))
                            .overlay(Text("Clip 1").font(.caption2).fontWeight(.bold).foregroundColor(.white))
                            .frame(width: totalWidth * 0.48, height: 42)

                        RoundedRectangle(cornerRadius: 6)
                            .fill(LinearGradient(colors: [.indigo, .indigo.opacity(0.7)], startPoint: .top, endPoint: .bottom))
                            .overlay(Text("Clip 2").font(.caption2).fontWeight(.bold).foregroundColor(.white))
                            .frame(width: totalWidth * 0.48, height: 42)
                    }

                    // Text / Subtitle Track
                    HStack {
                        Spacer().frame(width: totalWidth * 0.1)
                        RoundedRectangle(cornerRadius: 6)
                            .fill(LinearGradient(colors: [.purple, .purple.opacity(0.7)], startPoint: .top, endPoint: .bottom))
                            .overlay(Text("AI Captions").font(.caption2).fontWeight(.bold).foregroundColor(.white))
                            .frame(width: totalWidth * 0.4, height: 32)
                    }

                    // Audio Track 1
                    RoundedRectangle(cornerRadius: 6)
                        .fill(LinearGradient(colors: [.green, .green.opacity(0.7)], startPoint: .top, endPoint: .bottom))
                        .overlay(Text("ambient.wav").font(.caption2).fontWeight(.bold).foregroundColor(.white))
                        .frame(width: totalWidth * 0.96, height: 32)
                }
                .padding(.horizontal, 8)
                .padding(.vertical, 4)

                // Vertical Center Playhead Guide
                let progress = Double(bridge.playheadUs) / Double(bridge.durationUs)
                let playheadX = 8 + CGFloat(progress) * (totalWidth - 16)

                Rectangle()
                    .fill(Color.white)
                    .frame(width: 2.5, height: 122)
                    .offset(x: playheadX)
                    .overlay(
                        Circle()
                            .fill(Color.white)
                            .frame(width: 12, height: 12)
                            .offset(x: playheadX, y: -6),
                        alignment: .topLeading
                    )
            }
            .frame(width: totalWidth, height: 124)
            .contentShape(Rectangle())
            .gesture(
                DragGesture(minimumDistance: 0)
                    .onChanged { value in
                        let touchX = value.location.x - 8
                        let fraction = Double(touchX / (totalWidth - 16)).clamped(to: 0.0...1.0)
                        let targetUs = Int64(fraction * Double(bridge.durationUs))
                        bridge.seek(to: targetUs)
                    }
            )
            .gesture(
                MagnificationGesture()
                    .onChanged { scale in
                        currentZoom = (currentZoom * scale).clamped(to: 0.8...3.5)
                    }
            )
        }
    }
}

