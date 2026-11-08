package app.timewrap.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Slider
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TimePicker
import androidx.compose.material3.rememberTimePickerState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import app.timewrap.core.BlockStyle
import app.timewrap.core.colorPalette
import java.time.LocalTime

/**
 * La palette, demandée au cœur.
 *
 * Avant, elle était recopiée ici : deux listes qui devaient rester identiques
 * sans que rien ne le garantisse. Il n'y en a plus qu'une.
 */
val PALETTE: List<UInt> by lazy { colorPalette() }

/** Pastilles par ligne dans les sélecteurs. */
private const val SWATCHES_PER_ROW = 6

/** Une boîte à un seul champ de texte — création, renommage. */
@Composable
fun TextPromptDialog(
    title: String,
    hint: String,
    initial: String,
    confirm: String,
    onDismiss: () -> Unit,
    onConfirm: (String) -> Unit,
) {
    var value by remember { mutableStateOf(initial) }
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text(title) },
        text = {
            OutlinedTextField(
                value = value,
                onValueChange = { value = it },
                singleLine = true,
                label = { Text(hint) },
            )
        },
        confirmButton = { TextButton(onClick = { onConfirm(value) }) { Text(confirm) } },
        dismissButton = { TextButton(onClick = onDismiss) { Text("Annuler") } },
    )
}

/**
 * Vingt-quatre pastilles, et une couleur libre pour le reste.
 *
 * `onClear` n'apparaît que là où retirer la couleur a un sens — une valeur de
 * champ colorée peut redevenir neutre, la couleur de l'emploi du temps non.
 */
@Composable
fun ColorPickerDialog(
    title: String,
    current: UInt?,
    onDismiss: () -> Unit,
    onPick: (UInt) -> Unit,
    onClear: (() -> Unit)? = null,
    style: BlockStyle? = null,
    onStyle: ((BlockStyle) -> Unit)? = null,
) {
    var custom by remember { mutableStateOf(false) }
    // Le style se choisit sans fermer la boîte : on compare d'un coup d'œil.
    var shownStyle by remember(style) { mutableStateOf(style) }
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text(title) },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
                if (shownStyle != null && onStyle != null) {
                    Text("Style", style = MaterialTheme.typography.labelMedium)
                    StylePicker(
                        current = shownStyle!!,
                        accent = Color((current ?: PALETTE.first()).toInt()),
                    ) {
                        shownStyle = it
                        onStyle(it)
                    }
                    Text("Couleur", style = MaterialTheme.typography.labelMedium)
                }
                if (custom) {
                    FreeColorPicker(initial = current ?: PALETTE.first(), onPick = onPick)
                } else {
                    SwatchGrid(current = current, onPick = onPick)
                    TextButton(onClick = { custom = true }) { Text("Autre couleur…") }
                }
            }
        },
        confirmButton = {
            if (onClear != null) {
                TextButton(onClick = onClear) { Text("Retirer la couleur") }
            }
        },
        dismissButton = { TextButton(onClick = onDismiss) { Text("Fermer") } },
    )
}

/** Toute la palette, rangée en lignes égales. */
@Composable
fun SwatchGrid(current: UInt?, size: Int = 34, onPick: (UInt) -> Unit) {
    Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
        PALETTE.chunked(SWATCHES_PER_ROW).forEach { row ->
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                row.forEach { color ->
                    ColorDot(color, color == current, size = size) { onPick(color) }
                }
            }
        }
    }
}

/**
 * Une couleur au choix : teinte, intensité, luminosité.
 *
 * La luminosité est bornée : trop claire, la couleur disparaîtrait sur le fond
 * éclairci d'un bloc et l'intitulé deviendrait illisible.
 */
@Composable
private fun FreeColorPicker(initial: UInt, onPick: (UInt) -> Unit) {
    val start = remember(initial) {
        FloatArray(3).also { android.graphics.Color.colorToHSV(initial.toInt(), it) }
    }
    var hue by remember { mutableFloatStateOf(start[0]) }
    var saturation by remember { mutableFloatStateOf(start[1].coerceIn(0.25f, 1f)) }
    var value by remember { mutableFloatStateOf(start[2].coerceIn(0.3f, 0.85f)) }
    val color = android.graphics.Color.HSVToColor(floatArrayOf(hue, saturation, value)).toUInt()

    Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            ColorDot(color, selected = false, size = 44) {}
            Box(
                Modifier
                    .padding(start = 12.dp)
                    .clip(RoundedCornerShape(8.dp))
                    .background(Color(color.toInt()).copy(alpha = 0.24f))
                    .padding(horizontal = 12.dp, vertical = 8.dp),
            ) {
                Text("Aperçu d'un cours", fontWeight = FontWeight.SemiBold)
            }
        }
        Text("Teinte", style = MaterialTheme.typography.labelMedium)
        Slider(value = hue, onValueChange = { hue = it }, valueRange = 0f..359f)
        Text("Intensité", style = MaterialTheme.typography.labelMedium)
        Slider(value = saturation, onValueChange = { saturation = it }, valueRange = 0.25f..1f)
        Text("Luminosité", style = MaterialTheme.typography.labelMedium)
        Slider(value = value, onValueChange = { value = it }, valueRange = 0.3f..0.85f)
        TextButton(onClick = { onPick(color) }) { Text("Choisir cette couleur") }
    }
}

@Composable
fun ColorDot(color: UInt, selected: Boolean, size: Int = 44, onClick: () -> Unit) {
    Box(
        Modifier
            .size(size.dp)
            .clip(CircleShape)
            .background(Color(color.toInt()))
            .border(
                width = if (selected) 3.dp else 0.dp,
                color = MaterialTheme.colorScheme.onSurface,
                shape = CircleShape,
            )
            .clickable(onClick = onClick),
    )
}

/** Un sélecteur d'heure, partagé par l'éditeur d'événement et les réglages. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun TimePickerDialog(
    title: String,
    initial: LocalTime,
    onDismiss: () -> Unit,
    onConfirm: (LocalTime) -> Unit,
) {
    val state = rememberTimePickerState(
        initialHour = initial.hour,
        initialMinute = initial.minute,
        is24Hour = true,
    )
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text(title) },
        text = { TimePicker(state = state) },
        confirmButton = {
            TextButton(onClick = { onConfirm(LocalTime.of(state.hour, state.minute)) }) {
                Text("Choisir")
            }
        },
        dismissButton = { TextButton(onClick = onDismiss) { Text("Annuler") } },
    )
}

/** Une pastille de catégorie, telle qu'elle apparaît sur les blocs et listes. */
@Composable
fun CategoryChip(label: String, color: Color) {
    if (label.isBlank()) return
    Box(
        Modifier
            .clip(RoundedCornerShape(4.dp))
            .background(color.copy(alpha = 0.18f))
            .padding(horizontal = 5.dp, vertical = 1.dp),
    ) {
        Text(
            text = label,
            style = MaterialTheme.typography.labelSmall,
            fontWeight = FontWeight.Bold,
            color = color,
        )
    }
}
