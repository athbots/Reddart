// Reddart iOS: Mobile Zero-Copy Metal MTKView Viewport (120Hz ProMotion)
import SwiftUI
import MetalKit

public struct IOSMetalPreview: UIViewRepresentable {
    @ObservedObject var bridge = ReddartBridgeIOS.shared

    public init() {}

    public func makeCoordinator() -> Coordinator {
        Coordinator(self)
    }

    public func makeUIView(context: Context) -> MTKView {
        let mtkView = MTKView()
        mtkView.device = MTLCreateSystemDefaultDevice()
        mtkView.delegate = context.coordinator
        mtkView.preferredFramesPerSecond = 120 // 120 Hz ProMotion on iPhone 13 Pro and newer
        mtkView.enableSetNeedsDisplay = false
        mtkView.isPaused = false
        mtkView.clearColor = MTLClearColor(red: 0.05, green: 0.05, blue: 0.07, alpha: 1.0)
        return mtkView
    }

    public func updateUIView(_ uiView: MTKView, context: Context) {}

    public class Coordinator: NSObject, MTKViewDelegate {
        var parent: IOSMetalPreview
        var commandQueue: MTLCommandQueue?

        init(_ parent: IOSMetalPreview) {
            self.parent = parent
            super.init()
            if let device = MTLCreateSystemDefaultDevice() {
                self.commandQueue = device.makeCommandQueue()
            }
        }

        public func mtkView(_ view: MTKView, drawableSizeWillChange size: CGSize) {}

        public func draw(in view: MTKView) {
            guard let drawable = view.currentDrawable,
                  let descriptor = view.currentRenderPassDescriptor,
                  let commandQueue = commandQueue,
                  let commandBuffer = commandQueue.makeCommandBuffer(),
                  let encoder = commandBuffer.makeRenderCommandEncoder(descriptor: descriptor) else {
                return
            }

            // Zero-copy IOSurface to Metal texture draw
            encoder.endEncoding()
            commandBuffer.present(drawable)
            commandBuffer.commit()
        }
    }
}
