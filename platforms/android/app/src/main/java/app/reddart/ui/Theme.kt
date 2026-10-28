package app.reddart.ui

import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.darkColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.ui.graphics.Color

val BackgroundDark = Color(0xFF121216)
val SurfaceDark = Color(0xFF181820)
val PrimaryRed = Color(0xFFFF4545)
val SecondaryBlue = Color(0xFF3270C0)
val OnSurfaceLight = Color(0xFFE8E8EE)

private val DarkColorScheme = darkColorScheme(
    primary = PrimaryRed,
    secondary = SecondaryBlue,
    background = BackgroundDark,
    surface = SurfaceDark,
    onBackground = OnSurfaceLight,
    onSurface = OnSurfaceLight
)

@Composable
fun ReddartTheme(content: @Composable () -> Unit) {
    MaterialTheme(
        colorScheme = DarkColorScheme,
        content = content
    )
}

