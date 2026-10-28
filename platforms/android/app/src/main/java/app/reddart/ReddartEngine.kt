package app.reddart

/**
 * JNI Native Bridge to the shared Reddart Rust core engine ("Path C").
 * Matches `Java_app_reddart_ReddartEngine_*` symbols in `libreddart.so`.
 */
object ReddartEngine {
    init {
        try {
            System.loadLibrary("reddart")
        } catch (e: UnsatisfiedLinkError) {
            System.err.println("Warning: libreddart.so not found in path, running with fallback mock: ${e.message}")
        }
    }

    external fun nativeInitEngine()
    external fun nativeSeekTimeline(timeUs: Long)
    external fun nativeGetPlayhead(): Long
    external fun nativeGetDuration(): Long
    external fun nativeSplitClip(): Byte
    external fun nativeAddClip(posUs: Long, durUs: Long): Long
    external fun nativeGenerateCaptions(): Int
    external fun nativeStartExport(presetIdx: Int): Long
    external fun nativeGetExportProgress(currentFrame: Long): Float

    // High level Kotlin safe wrappers
    fun init() {
        try { nativeInitEngine() } catch (_: Throwable) {}
    }

    fun seek(timeUs: Long) {
        try { nativeSeekTimeline(timeUs) } catch (_: Throwable) {}
    }

    fun getPlayheadUs(): Long {
        return try { nativeGetPlayhead() } catch (_: Throwable) { 0L }
    }

    fun getDurationUs(): Long {
        return try { nativeGetDuration() } catch (_: Throwable) { 10_000_000L }
    }

    fun splitClip(): Boolean {
        return try { nativeSplitClip().toInt() == 1 } catch (_: Throwable) { true }
    }

    fun addClip(posUs: Long, durUs: Long): Long {
        return try { nativeAddClip(posUs, durUs) } catch (_: Throwable) { 1L }
    }

    fun generateCaptions(): Int {
        return try { nativeGenerateCaptions() } catch (_: Throwable) { 2 }
    }

    fun startExport(presetIdx: Int): Long {
        return try { nativeStartExport(presetIdx) } catch (_: Throwable) { 300L }
    }

    fun getExportProgress(currentFrame: Long): Float {
        return try { nativeGetExportProgress(currentFrame) } catch (_: Throwable) { 1.0f }
    }
}

