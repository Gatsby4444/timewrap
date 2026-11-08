package app.timewrap.ui

import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.drawscope.clipRect
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import app.timewrap.core.BlockStyle

/** Le nom d'un style, tel que l'utilisateur le choisit. */
fun BlockStyle.label(): String = when (this) {
    BlockStyle.SOLID -> "Plein"
    BlockStyle.STRIPES -> "Rayé"
    BlockStyle.OUTLINE -> "Contour"
    BlockStyle.DOTS -> "Pointillé"
}

/**
 * Le fond d'un bloc de cours, selon son style.
 *
 * Seule la présentation se décide ici ; quel style revient à quelle séance,
 * c'est le cœur qui l'a dit. Tous les styles gardent un fond clair derrière le
 * texte : un motif ne doit jamais rendre l'intitulé illisible.
 *
 * Une épreuve reçoit en plus un cadre épais, quel que soit son style : elle doit
 * se voir de loin dans la semaine.
 */
fun Modifier.blockSurface(
    style: BlockStyle,
    accent: Color,
    faded: Boolean = false,
    exam: Boolean = false,
    shape: RoundedCornerShape = RoundedCornerShape(8.dp),
): Modifier {
    val strength = if (faded) 0.5f else 1f
    val fill = when (style) {
        BlockStyle.SOLID -> 0.24f
        BlockStyle.STRIPES -> 0.12f
        BlockStyle.OUTLINE -> 0.05f
        BlockStyle.DOTS -> 0.10f
    } * strength

    var modifier = this
        .clip(shape)
        .drawBehind {
            drawRect(accent.copy(alpha = fill))
            when (style) {
                BlockStyle.STRIPES -> {
                    // Des diagonales espacées : assez fines pour lire par-dessus.
                    val step = 9.dp.toPx()
                    val stroke = 2.5.dp.toPx()
                    val stripe = accent.copy(alpha = 0.22f * strength)
                    clipRect {
                        var x = -size.height
                        while (x < size.width) {
                            drawLine(
                                color = stripe,
                                start = Offset(x, size.height),
                                end = Offset(x + size.height, 0f),
                                strokeWidth = stroke,
                            )
                            x += step
                        }
                    }
                }
                BlockStyle.DOTS -> {
                    val step = 8.dp.toPx()
                    val radius = 1.4.dp.toPx()
                    val dot = accent.copy(alpha = 0.38f * strength)
                    var y = step / 2
                    var row = 0
                    while (y < size.height) {
                        var x = if (row % 2 == 0) step / 2 else step
                        while (x < size.width) {
                            drawCircle(dot, radius, Offset(x, y))
                            x += step
                        }
                        y += step
                        row++
                    }
                }
                BlockStyle.SOLID, BlockStyle.OUTLINE -> Unit
            }
        }

    val borderWidth: Dp = when {
        exam -> 2.5.dp
        style == BlockStyle.OUTLINE -> 1.5.dp
        else -> 0.dp
    }
    if (borderWidth > 0.dp) {
        modifier = modifier.border(borderWidth, accent.copy(alpha = strength), shape)
    }
    return modifier
}

/** « EXAMEN », en petit, en haut d'un bloc. */
@Composable
fun ExamBadge(accent: Color) {
    Box(
        Modifier
            .clip(RoundedCornerShape(4.dp))
            .blockSurface(BlockStyle.SOLID, accent)
            .padding(horizontal = 5.dp, vertical = 1.dp),
    ) {
        Text(
            text = "EXAMEN",
            fontSize = 9.sp,
            fontWeight = FontWeight.Bold,
            color = accent,
            letterSpacing = 0.6.sp,
        )
    }
}

/** Un petit échantillon du style, pour le choisir. */
@Composable
fun StyleSwatch(style: BlockStyle, accent: Color, selected: Boolean, onClick: () -> Unit) {
    Column(
        horizontalAlignment = Alignment.CenterHorizontally,
        modifier = Modifier
            .clip(RoundedCornerShape(10.dp))
            .clickable(onClick = onClick)
            .padding(4.dp),
    ) {
        Box(
            Modifier
                .size(width = 56.dp, height = 40.dp)
                .blockSurface(style, accent)
                .then(
                    if (selected) {
                        Modifier.border(
                            2.dp,
                            MaterialTheme.colorScheme.onSurface,
                            RoundedCornerShape(8.dp),
                        )
                    } else {
                        Modifier
                    },
                ),
        )
        Text(
            text = style.label(),
            style = MaterialTheme.typography.labelSmall,
            fontWeight = if (selected) FontWeight.Bold else FontWeight.Normal,
            modifier = Modifier.padding(top = 2.dp),
        )
    }
}

/** Les quatre styles côte à côte. */
@Composable
fun StylePicker(current: BlockStyle, accent: Color, onPick: (BlockStyle) -> Unit) {
    Row(horizontalArrangement = Arrangement.spacedBy(4.dp)) {
        BlockStyle.entries.forEach { style ->
            StyleSwatch(style, accent, style == current) { onPick(style) }
        }
    }
}
