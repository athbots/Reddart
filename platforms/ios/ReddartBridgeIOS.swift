// Reddart iOS: Swift Bridge to Shared Native Rust Core (Path C)
import Foundation
import Combine
import UIKit

public final class ReddartBridgeIOS: ObservableObject {
    public static let shared = ReddartBridgeIOS()

    private var enginePtr: OpaquePointer?

    @Published public var projectName: String = "Reddart Mobile Project"
    @Published public var playheadUs: Int64 = 0
    @Published public var durationUs: Int64 = 8_000_000
    @Published public var isPlaying: Bool = false
    @Published public var activeLut: String = "Natural Rec.709"
    @Published public var exportProgress: Float = 0.0
    @Published public var isExporting: Bool = false
    @Published public var statusMessage: String = "Ready"

    private var displayTimer: Timer?
    private let impactFeedback = UIImpactFeedbackGenerator(style: .light)

    private init() {
        initEngine()
        impactFeedback.prepare()
    }

    deinit {
        destroyEngine()
    }

    public func initEngine() {
        enginePtr = reddart_engine_create()
        refreshState()
    }

    public func destroyEngine() {
        displayTimer?.invalidate()
        if let ptr = enginePtr {
            reddart_engine_destroy(ptr)
            enginePtr = nil
        }
    }

    public func seek(to timestampUs: Int64) {
        guard let ptr = enginePtr else { return }
        let clamped = max(0, min(timestampUs, durationUs))
        reddart_engine_seek(ptr, clamped)
        playheadUs = clamped
    }

    public func triggerHaptic() {
        impactFeedback.impactOccurred()
    }

    public func togglePlayback() {
        isPlaying.toggle()
        triggerHaptic()
        if isPlaying {
            displayTimer = Timer.scheduledTimer(withTimeInterval: 1.0 / 120.0, repeats: true) { [weak self] _ in
                guard let self = self else { return }
                var next = self.playheadUs + 8333 // 8.33ms at 120Hz ProMotion
                if next >= self.durationUs {
                    next = 0
                }
                self.seek(to: next)
            }
        } else {
            displayTimer?.invalidate()
            displayTimer = nil
        }
    }

    public func splitClip() {
        guard let ptr = enginePtr else { return }
        triggerHaptic()
        let res = reddart_engine_split_clip(ptr)
        if res == 1 {
            statusMessage = "Clip split"
            refreshState()
        }
    }

    public func deleteClip() {
        guard let ptr = enginePtr else { return }
        triggerHaptic()
        let res = reddart_engine_delete_selected(ptr)
        if res == 1 {
            statusMessage = "Clip deleted"
            refreshState()
        }
    }

    public func undo() {
        guard let ptr = enginePtr else { return }
        triggerHaptic()
        if reddart_engine_undo(ptr) == 1 {
            statusMessage = "Undo"
            refreshState()
        }
    }

    public func redo() {
        guard let ptr = enginePtr else { return }
        triggerHaptic()
        if reddart_engine_redo(ptr) == 1 {
            statusMessage = "Redo"
            refreshState()
        }
    }

    public func applyLut(_ name: String) {
        guard let ptr = enginePtr else { return }
        triggerHaptic()
        name.withCString { cStr in
            if reddart_engine_apply_lut(ptr, cStr) == 1 {
                activeLut = name
                statusMessage = "LUT: \(name)"
            }
        }
    }

    public func generateAiCaptions() {
        guard let ptr = enginePtr else { return }
        triggerHaptic()
        statusMessage = "Whisper Neural Engine processing..."
        if let jsonPtr = reddart_engine_generate_captions(ptr) {
            reddart_string_free(jsonPtr)
            statusMessage = "Captions generated"
            refreshState()
        }
    }

    public func denoiseAudio() {
        guard let ptr = enginePtr else { return }
        triggerHaptic()
        statusMessage = "Spectral Gating active"
        _ = reddart_engine_denoise_audio(ptr)
    }

    public func autoReframe() {
        guard let ptr = enginePtr else { return }
        triggerHaptic()
        statusMessage = "Reframing 16:9 to 9:16..."
        if let resPtr = reddart_engine_auto_reframe(ptr, 9.0, 16.0) {
            reddart_string_free(resPtr)
            statusMessage = "Reframe completed"
        }
    }

    public func startExport(preset: String) {
        guard let ptr = enginePtr else { return }
        isExporting = true
        exportProgress = 0.05
        triggerHaptic()
        preset.withCString { pStr in
            "out.mp4".withCString { oStr in
                _ = reddart_engine_start_export(ptr, pStr, oStr)
            }
        }

        Timer.scheduledTimer(withTimeInterval: 0.1, repeats: true) { [weak self] timer in
            guard let self = self, let p = self.enginePtr else {
                timer.invalidate()
                return
            }
            let progress = reddart_engine_get_export_progress(p)
            self.exportProgress = progress
            if progress >= 1.0 {
                timer.invalidate()
                self.isExporting = false
                self.statusMessage = "Export complete"
            }
        }
    }

    public func refreshState() {
        guard let ptr = enginePtr else { return }
        if let cJson = reddart_engine_get_state_json(ptr) {
            let jsonStr = String(cString: cJson)
            reddart_string_free(cJson)
            if let data = jsonStr.data(using: .utf8),
               let obj = try? JSONSerialization.jsonObject(with: data) as? [String: Any] {
                if let name = obj["name"] as? String { projectName = name }
                if let ph = obj["playhead_us"] as? Int64 { playheadUs = ph }
                if let dur = obj["duration_us"] as? Int64 { durationUs = max(dur, 1) }
                if let lut = obj["active_lut"] as? String { activeLut = lut }
            }
        }
    }

    public func formatTime(_ us: Int64) -> String {
        let totalSeconds = Double(us) / 1_000_000.0
        let minutes = Int(totalSeconds / 60)
        let seconds = Int(totalSeconds.truncatingRemainder(dividingBy: 60))
        let frames = Int((totalSeconds.truncatingRemainder(dividingBy: 1.0)) * 60)
        return String(format: "%02d:%02d:%02d", minutes, seconds, frames)
    }
}
