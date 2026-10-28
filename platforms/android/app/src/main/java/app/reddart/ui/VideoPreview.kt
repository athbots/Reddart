package app.reddart.ui

import android.view.SurfaceView
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.ui.viewinterop.AndroidView

/**
 * Mobile Video Preview Viewport.
 * Uses a hardware SurfaceView attached to the Vulkan / wgpu compositor swapchain.
 */
@Composable
fun VideoPreview(
    isPlaying: Boolean,
    playheadUs: Long,
    modifier: Modifier = Modifier
) {
    Box(
        modifier = modifier
            .fillMaxWidth()
            .aspectRatio(9f / 16f) // 9:16 vertical video default
            .clip(RoundedCornerShape(12.dp))
            .background(Color(0xFF0A0A0C)),
        contentAlignment = Alignment.Center
    ) {
        AndroidView(
            factory = { context ->
                SurfaceView(context).apply {
                    // Surface will receive decoded frames via AHardwareBuffer
                }
            },
            modifier = Modifier.matchParentSize()
        )

        // Overlay status badges
        Text(
            text = "1080×1920 | 60 FPS",
            color = Color.LightGray,
            fontSize = 11.sp,
            modifier = Modifier
                .align(Alignment.TopStart)
                .padding(10.dp)
        )

        Text(
            text = if (isPlaying) "● PLAYING" else "⏸ PAUSED",
            color = if (isPlaying) Color(0xFF50DC64) else Color(0xFFDCB450),
            fontSize = 11.sp,
            modifier = Modifier
                .align(Alignment.TopEnd)
                .padding(10.dp)
        )

        Text(
            text = String.format(
                "%02d:%02d.%03d",
                (playheadUs / 60_000_000),
                (playheadUs / 1_000_000) % 60,
                (playheadUs / 1_000) % 1_000
            ),
            color = Color.White,
            fontSize = 14.sp,
            modifier = Modifier
                .align(Alignment.BottomCenter)
                .padding(bottom = 12.dp)
        )
    }
}

