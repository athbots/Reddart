package app.reddart

import android.content.Intent
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import app.reddart.service.ExportForegroundService
import app.reddart.ui.EditorScreen
import app.reddart.ui.ReddartTheme

class MainActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()

        setContent {
            ReddartTheme {
                EditorScreen(
                    onStartExport = { presetIdx ->
                        val serviceIntent = Intent(this, ExportForegroundService::class.java).apply {
                            putExtra("PRESET_INDEX", presetIdx)
                        }
                        startService(serviceIntent)
                    }
                )
            }
        }
    }
}

