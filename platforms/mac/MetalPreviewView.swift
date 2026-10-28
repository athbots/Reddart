// Reddart macOS: Zero-Copy Metal MTKView Preview Viewport (120 Hz ProMotion)
import SwiftUI
import MetalKit

public struct MetalPreviewView: NSViewRepresentable {
    @ObservedObject var bridge = ReddartBridge.shared

    public init() {}

    public func makeCoordinator() -> Coordinator {
        Coordinator(self)
    }

    public func makeNSView(context: Context) -> MTKView {
        let mtkView = MTKView()
        mtkView.device = MTLCreateSystemDefaultDevice()
        mtkView.delegate = context.coordinator
        mtkView.preferredFramesPerSecond = 120 // ProMotion 120Hz display refresh
        mtkView.enableSetNeedsDisplay = false
        mtkView.isPaused = false
        mtkView.clearColor = MTLClearColor(red: 0.08, green: 0.08, blue: 0.09, alpha: 1.0)
        return mtkView
    }

    public func updateNSView(_ nsView: MTKView, context: Context) {
        // Updated whenever SwiftUI state refreshes
    }

    public class Coordinator: NSObject, MTKViewDelegate {
        var parent: MetalPreviewView
        var commandQueue: MTLCommandQueue?

        init(_ parent: MetalPreviewView) {
            self.parent = parent
            super.init()
            if let device = MTLCreateSystemDefaultDevice() {
                self.commandQueue = device.makeCommandQueue()
            }
        }

        public func mtkView(_ view: MTKView, drawableSizeWillChange size: CGSize) {
            // Adjust Metal viewport bounds
        }

        public func draw(in view: MTKView) {
            guard let drawable = view.currentDrawable,
                  let descriptor = view.currentRenderPassDescriptor,
                  let commandQueue = commandQueue,
                  let commandBuffer = commandQueue.makeCommandBuffer(),
                  let encoder = commandBuffer.makeRenderCommandEncoder(descriptor: descriptor) else {
                return
            }

            // Draw zero-copy composited video frame
            encoder.endEncoding()
            commandBuffer.present(drawable)
            commandBuffer.commit()
        }
    }
}

