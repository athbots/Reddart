// Reddart macOS: Swift FFI Bridge to Shared Rust Core (Path C)
import Foundation
import Combine

public final class ReddartBridge: ObservableObject {
    public static let shared = ReddartBridge()

    private var enginePtr: OpaquePointer?

    @Published public var projectName: String = "Reddart Mac Project"
    @Published public var playheadUs: Int64 = 0
    @Published public var durationUs: Int64 = 8_000_000
    @Published public var isPlaying: Bool = false
    @Published public var activeLut: String = "Natural Rec.709"
    @Published public var exportProgress: Float = 0.0
    @Published public var isExporting: Bool = false
    @Published public var trackCount: Int = 3
    @Published public var statusMessage: String = "Ready"

    private var displayTimer: Timer?

    private init() {
        initEngine()
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

    public func togglePlayback() {
        isPlaying.toggle()
        if isPlaying {
            displayTimer = Timer.scheduledTimer(withTimeInterval: 1.0 / 120.0, repeats: true) { [weak self] _ in
                guard let self = self else { return }
                var next = self.playheadUs + 8333 // 8.33ms per frame at 120Hz ProMotion
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
        let res = reddart_engine_split_clip(ptr)
        if res == 1 {
            statusMessage = "Clip split at \(formatTime(playheadUs))"
            refreshState()
        }
    }

    public func deleteSelectedClip() {
        guard let ptr = enginePtr else { return }
        let res = reddart_engine_delete_selected(ptr)
        if res == 1 {
            statusMessage = "Selected clip removed"
            refreshState()
        }
    }

    public func undo() {
        guard let ptr = enginePtr else { return }
        if reddart_engine_undo(ptr) == 1 {
            statusMessage = "Undo performed"
            refreshState()
        }
    }

    public func redo() {
        guard let ptr = enginePtr else { return }
        if reddart_engine_redo(ptr) == 1 {
            statusMessage = "Redo performed"
            refreshState()
        }
    }

    public func applyLut(_ name: String) {
        guard let ptr = enginePtr else { return }
        name.withCString { cStr in
            if reddart_engine_apply_lut(ptr, cStr) == 1 {
                activeLut = name
                statusMessage = "Applied 3D LUT: \(name)"
            }
        }
    }

    public func generateAiCaptions() {
        guard let ptr = enginePtr else { return }
        statusMessage = "Aligning captions with on-device Whisper..."
        if let jsonPtr = reddart_engine_generate_captions(ptr) {
            let jsonString = String(cString: jsonPtr)
            reddart_string_free(jsonPtr)
            statusMessage = "AI Captions generated successfully"
            refreshState()
        }
    }

    public func denoiseAudio() {
        guard let ptr = enginePtr else { return }
        statusMessage = "Applying Spectral Gating denoiser..."
        if reddart_engine_denoise_audio(ptr) == 1 {
            statusMessage = "Audio background noise attenuated (-15 dB)"
        }
    }

    public func autoReframe(aspectW: Float, aspectH: Float) {
        guard let ptr = enginePtr else { return }
        statusMessage = "Computing auto-reframe smooth camera trajectory..."
        if let resPtr = reddart_engine_auto_reframe(ptr, aspectW, aspectH) {
            let resStr = String(cString: resPtr)
            reddart_string_free(resPtr)
            statusMessage = "Auto-Reframe solved for \(Int(aspectW)):\(Int(aspectH))"
        }
    }

    public func startExport(preset: String) {
        guard let ptr = enginePtr else { return }
        isExporting = true
        exportProgress = 0.05
        statusMessage = "Starting VideoToolbox hardware export: \(preset)..."
        preset.withCString { pStr in
            "output.mp4".withCString { oStr in
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
                self.statusMessage = "Export complete (ProRes/H.264 VideoToolbox)"
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
                if let tc = obj["tracks_count"] as? Int { trackCount = tc }
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

