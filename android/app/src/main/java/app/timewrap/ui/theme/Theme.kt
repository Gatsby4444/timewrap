package app.timewrap.ui.theme

import android.os.Build
import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.dynamicDarkColorScheme
import androidx.compose.material3.dynamicLightColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext

private val Ink = Color(0xFF1B1B2F)
private val Periwinkle = Color(0xFF4C5FD5)
private val PeriwinkleLight = Color(0xFF8AB4FF)
private val Sand = Color(0xFFF6F5F2)

private val LightColors = lightColorScheme(
    primary = Periwinkle,
    onPrimary = Color.White,
    secondary = Color(0xFF5B6478),
    background = Sand,
    onBackground = Ink,
    surface = Color.White,
    onSurface = Ink,
)

private val DarkColors = darkColorScheme(
    primary = PeriwinkleLight,
    onPrimary = Ink,
    secondary = Color(0xFFB9C0D4),
    background = Ink,
    onBackground = Color(0xFFECECF2),
    surface = Color(0xFF25253D),
    onSurface = Color(0xFFECECF2),
)

@Composable
fun TimewrapTheme(
    darkTheme: Boolean = isSystemInDarkTheme(),
    content: @Composable () -> Unit,
) {
    val colors = when {
        // Material You : on suit le fond d'ecran quand le systeme sait le faire.
        Build.VERSION.SDK_INT >= Build.VERSION_CODES.S -> {
            val context = LocalContext.current
            if (darkTheme) dynamicDarkColorScheme(context) else dynamicLightColorScheme(context)
        }
        darkTheme -> DarkColors
        else -> LightColors
    }

    MaterialTheme(colorScheme = colors, content = content)
}
