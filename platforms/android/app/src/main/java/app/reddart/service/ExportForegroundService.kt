package app.reddart.service

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.Service
import android.content.Context
import android.content.Intent
import android.os.Build
import android.os.IBinder
import androidx.core.app.NotificationCompat
import app.reddart.R
import app.reddart.ReddartEngine
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch

/**
 * Foreground service to execute hardware exports without OS kills.
 * Section 3.10 of the Technical Proposal.
 */
class ExportForegroundService : Service() {
    private val scope = CoroutineScope(Dispatchers.Default + Job())
    private val notificationId = 1001
    private val channelId = "reddart_export_channel"

    override fun onCreate() {
        super.onCreate()
        createNotificationChannel()
    }

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        val presetIdx = intent?.getIntExtra("PRESET_INDEX", 0) ?: 0
        val totalFrames = ReddartEngine.startExport(presetIdx)

        startForeground(notificationId, buildNotification(0f))

        scope.launch {
            var frame = 0L
            while (frame < totalFrames) {
                delay(100)
                frame += 15 // Simulated render throughput
                val progress = ReddartEngine.getExportProgress(frame)
                updateNotification(progress)
            }
            stopForeground(STOP_FOREGROUND_REMOVE)
            stopSelf()
        }

        return START_NOT_STICKY
    }

    override fun onBind(intent: Intent?): IBinder? = null

    private fun createNotificationChannel() {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            val channel = NotificationChannel(
                channelId,
                getString(R.string.export_channel_name),
                NotificationManager.IMPORTANCE_LOW
            ).apply {
                description = getString(R.string.export_channel_description)
            }
            val manager = getSystemService(Context.NOTIFICATION_SERVICE) as NotificationManager
            manager.createNotificationChannel(channel)
        }
    }

    private fun buildNotification(progressFraction: Float): Notification {
        val pct = (progressFraction * 100).toInt()
        return NotificationCompat.Builder(this, channelId)
            .setContentTitle("Exporting Video")
            .setContentText("Rendering frames: $pct%")
            .setSmallIcon(R.drawable.ic_launcher)
            .setProgress(100, pct, false)
            .setOngoing(true)
            .build()
    }

    private fun updateNotification(progressFraction: Float) {
        val manager = getSystemService(Context.NOTIFICATION_SERVICE) as NotificationManager
        manager.notify(notificationId, buildNotification(progressFraction))
    }
}

