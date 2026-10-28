// Reddart macOS: App Entry Point & System Menus
import SwiftUI

@main
struct ReddartApp: App {
    @StateObject private var bridge = ReddartBridge.shared

    var body: some Scene {
        WindowGroup {
            ContentView()
                .environmentObject(bridge)
        }
        .windowStyle(.hiddenTitleBar)
        .commands {
            // Edit Menu
            CommandGroup(replacing: .undoRedo) {
                Button("Undo") {
                    bridge.undo()
                }
                .keyboardShortcut("z", modifiers: .command)

                Button("Redo") {
                    bridge.redo()
                }
                .keyboardShortcut("z", modifiers: [.command, .shift])
            }

            // Timeline & Clip Menu
            CommandMenu("Clip") {
                Button("Split Clip at Playhead") {
                    bridge.splitClip()
                }
                .keyboardShortcut("b", modifiers: .command)

                Button("Delete Clip") {
                    bridge.deleteSelectedClip()
                }
                .keyboardShortcut(.delete, modifiers: [])

                Divider()

                Button("Auto Captions (Whisper)") {
                    bridge.generateAiCaptions()
                }

                Button("Denoise Audio") {
                    bridge.denoiseAudio()
                }

                Button("Auto-Reframe to 9:16") {
                    bridge.autoReframe(aspectW: 9, aspectH: 16)
                }
            }

            // Playback Menu
            CommandMenu("Playback") {
                Button(bridge.isPlaying ? "Pause" : "Play") {
                    bridge.togglePlayback()
                }
                .keyboardShortcut(.space, modifiers: [])

                Button("Jump to Start") {
                    bridge.seek(to: 0)
                }
                .keyboardShortcut(.leftArrow, modifiers: .command)

                Button("Jump to End") {
                    bridge.seek(to: bridge.durationUs)
                }
                .keyboardShortcut(.rightArrow, modifiers: .command)
            }
        }
    }
}
