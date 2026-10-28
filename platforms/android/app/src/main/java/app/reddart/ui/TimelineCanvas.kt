package app.reddart.ui

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.gestures.detectDragGestures
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.unit.dp

/**
 * 120 Hz Native Timeline Canvas.
 * Custom GPU-drawn Canvas matching Section 3.6 of the proposal.
 * Eliminates garbage-collected view tree allocations during dragging and scrubbing.
 */
@Composable
fun TimelineCanvas(
    playheadUs: Long,
    totalDurationUs: Long,
    onSeek: (Long) -> Unit,
    modifier: Modifier = Modifier
) {
    val zoomPxPerSec = 80f // 80 pixels per second
    val rulerHeight = 28f
    val laneHeight = 52f

    Canvas(
        modifier = modifier
            .fillMaxWidth()
            .height(240.dp)
            .pointerInput(totalDurationUs) {
                detectDragGestures { change, dragAmount ->
                    change.consume()
                    val deltaUs = (dragAmount.x / zoomPxPerSec * 1_000_000f).toLong()
                    val newPlayhead = (playheadUs + deltaUs).coerceIn(0L, totalDurationUs)
                    onSeek(newPlayhead)
                }
            }
    ) {
        val width = size.width
        val height = size.height

        // 1. Background
        drawRect(Color(0xFF141418), size = Size(width, height))

        // 2. Time Ruler ticks
        drawRect(Color(0xFF1E1E26), size = Size(width, rulerHeight))
        val totalSecs = (width / zoomPxPerSec).toInt() + 1
        for (sec in 0..totalSecs) {
            val x = sec * zoomPxPerSec
            val isMajor = sec % 5 == 0
            val tickH = if (isMajor) 14f else 7f
            drawLine(
                color = if (isMajor) Color(0xFF9090A0) else Color(0xFF505060),
                start = Offset(x, rulerHeight - tickH),
                end = Offset(x, rulerHeight),
                strokeWidth = 1.5f
            )
        }

        // 3. Track 1: Video Lane
        val track1Y = rulerHeight + 6f
        drawRect(Color(0xFF1A2230), topLeft = Offset(0f, track1Y), size = Size(width, laneHeight))
        // Sample Video Clip 1
        drawRoundRect(
            color = Color(0xFF234B82),
            topLeft = Offset(0f, track1Y + 4f),
            size = Size(4f * zoomPxPerSec, laneHeight - 8f),
            cornerRadius = androidx.compose.ui.geometry.CornerRadius(6f, 6f)
        )
        // Sample Video Clip 2
        drawRoundRect(
            color = Color(0xFF2C5E9E),
            topLeft = Offset(4f * zoomPxPerSec + 4f, track1Y + 4f),
            size = Size(6f * zoomPxPerSec, laneHeight - 8f),
            cornerRadius = androidx.compose.ui.geometry.CornerRadius(6f, 6f)
        )

        // 4. Track 2: Audio Lane
        val track2Y = track1Y + laneHeight + 6f
        drawRect(Color(0xFF16241A), topLeft = Offset(0f, track2Y), size = Size(width, laneHeight))
        drawRoundRect(
            color = Color(0xFF28643C),
            topLeft = Offset(0f, track2Y + 4f),
            size = Size(10f * zoomPxPerSec, laneHeight - 8f),
            cornerRadius = androidx.compose.ui.geometry.CornerRadius(6f, 6f)
        )

        // 5. Track 3: Subtitles / Text Lane
        val track3Y = track2Y + laneHeight + 6f
        drawRect(Color(0xFF241C16), topLeft = Offset(0f, track3Y), size = Size(width, laneHeight))
        drawRoundRect(
            color = Color(0xFF784E26),
            topLeft = Offset(1f * zoomPxPerSec, track3Y + 4f),
            size = Size(3f * zoomPxPerSec, laneHeight - 8f),
            cornerRadius = androidx.compose.ui.geometry.CornerRadius(6f, 6f)
        )

        // 6. Center Playhead Red Needle
        val playheadX = (playheadUs / 1_000_000f) * zoomPxPerSec
        drawLine(
            color = Color(0xFFFF4545),
            start = Offset(playheadX, 0f),
            end = Offset(playheadX, height),
            strokeWidth = 3f
        )
        // Playhead diamond head
        drawCircle(
            color = Color(0xFFFF4545),
            radius = 8f,
            center = Offset(playheadX, 8f)
        )
    }
}

