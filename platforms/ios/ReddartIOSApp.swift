// Reddart iOS: Application Entry Point
import SwiftUI

@main
struct ReddartIOSApp: App {
    @StateObject private var bridge = ReddartBridgeIOS.shared

    var body: some Scene {
        WindowGroup {
            IOSEditorView()
                .environmentObject(bridge)
                .preferredColorScheme(.dark)
        }
    }
}

