package app.timewrap.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.Add
import androidx.compose.material.icons.outlined.KeyboardArrowDown
import androidx.compose.material.icons.outlined.KeyboardArrowUp
import androidx.compose.material.icons.outlined.MoreVert
import androidx.compose.material.icons.outlined.Warning
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import app.timewrap.UiState
import app.timewrap.core.Calendar
import app.timewrap.core.CalendarKind
import app.timewrap.core.CalendarSummary

/**
 * La palette du cœur, répétée ici pour le sélecteur de couleur.
 *
 * Les deux listes doivent rester identiques : c'est le cœur qui attribue la
 * couleur par défaut d'un agenda, l'interface qui permet d'en changer.
 */
val PALETTE: List<UInt> = listOf(
    0xFF4C5FD5u, 0xFF2E9E7Au, 0xFFD2694Bu, 0xFF8155C6u,
    0xFF3C87C8u, 0xFFC2528Au, 0xFF7A8A3Cu, 0xFFB08236u,
)

/**
 * L'écran d'accueil : les agendas comme autant de dossiers.
 *
 * On y entre pour choisir ce qu'on veut voir — les cours seuls, le personnel
 * seul, ou tout ensemble — et c'est aussi de là que partent les deux
 * gestionnaires, celui des chevauchements et celui des règles visuelles.
 */
@Composable
fun HomeScreen(
    state: UiState,
    onOpen: (String?) -> Unit,
    onImport: () -> Unit,
    onCreate: (String) -> Unit,
    onRename: (String, String) -> Unit,
    onColor: (String, UInt) -> Unit,
    onToggle: (String, Boolean) -> Unit,
    onMove: (String, Int) -> Unit,
    onDelete: (String) -> Unit,
    onConflicts: () -> Unit,
    onRules: () -> Unit,
) {
    var creating by remember { mutableStateOf(false) }
    var renaming by remember { mutableStateOf<Calendar?>(null) }
    var recoloring by remember { mutableStateOf<Calendar?>(null) }
    var deleting by remember { mutableStateOf<Calendar?>(null) }

    LazyColumn(
        modifier = Modifier.fillMaxSize(),
        contentPadding = PaddingValues(16.dp),
        verticalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        item {
            Text(
                text = "Mes agendas",
                style = MaterialTheme.typography.headlineSmall,
                fontWeight = FontWeight.Bold,
            )
            Text(
                text = "Chaque agenda est un dossier : ouvrez-en un pour ne voir que lui.",
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onBackground.copy(alpha = 0.6f),
            )
        }

        item { OverviewCard(state, onOpen = { onOpen(null) }) }

        itemsIndexed(state.summaries, key = { _, summary -> summary.calendar.id }) { index, summary ->
            CalendarFolder(
                summary = summary,
                isFirst = index == 0,
                isLast = index == state.summaries.lastIndex,
                onOpen = { onOpen(summary.calendar.id) },
                onRename = { renaming = summary.calendar },
                onColor = { recoloring = summary.calendar },
                onToggle = { onToggle(summary.calendar.id, it) },
                onUp = { onMove(summary.calendar.id, index - 1) },
                onDown = { onMove(summary.calendar.id, index + 1) },
                onDelete = { deleting = summary.calendar },
            )
        }

        if (state.summaries.isEmpty()) {
            item {
                Text(
                    text = "Aucun agenda pour l'instant. Importez l'export .ics de votre ENT, " +
                        "ou créez un dossier vide pour vos propres créneaux.",
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.onBackground.copy(alpha = 0.7f),
                )
            }
        }

        item {
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                OutlinedButton(onClick = { creating = true }, modifier = Modifier.weight(1f)) {
                    Icon(Icons.Outlined.Add, contentDescription = null, Modifier.size(18.dp))
                    Spacer(Modifier.width(6.dp))
                    Text("Nouvel agenda")
                }
                OutlinedButton(onClick = onImport, modifier = Modifier.weight(1f)) {
                    Text("Importer .ics")
                }
            }
        }

        item {
            Spacer(Modifier.height(8.dp))
            SectionLabel("Gestion")
        }

        item {
            ManagerRow(
                title = "Chevauchements",
                subtitle = when (state.totalConflicts) {
                    0 -> "Aucun créneau ne se marche dessus."
                    1 -> "1 chevauchement à arbitrer."
                    else -> "${state.totalConflicts} chevauchements à arbitrer."
                },
                alert = state.totalConflicts > 0,
                onClick = onConflicts,
            )
        }

        item {
            ManagerRow(
                title = "Règles visuelles",
                subtitle = if (state.rules.isEmpty()) {
                    "Colorier d'un coup tous les CM, tous les TP…"
                } else {
                    "${state.rules.size} règle(s), ${state.categories.size} catégorie(s)."
                },
                alert = false,
                onClick = onRules,
            )
        }
    }

    if (creating) {
        TextPromptDialog(
            title = "Nouvel agenda",
            hint = "Nom",
            initial = "",
            confirm = "Créer",
            onDismiss = { creating = false },
            onConfirm = { name ->
                if (name.isNotBlank()) onCreate(name)
                creating = false
            },
        )
    }

    renaming?.let { calendar ->
        TextPromptDialog(
            title = "Renommer l'agenda",
            hint = "Nom",
            initial = calendar.name,
            confirm = "Enregistrer",
            onDismiss = { renaming = null },
            onConfirm = { name ->
                onRename(calendar.id, name.trim().ifBlank { calendar.name })
                renaming = null
            },
        )
    }

    recoloring?.let { calendar ->
        ColorPickerDialog(
            current = calendar.color,
            onDismiss = { recoloring = null },
            onPick = {
                onColor(calendar.id, it)
                recoloring = null
            },
        )
    }

    deleting?.let { calendar ->
        AlertDialog(
            onDismissRequest = { deleting = null },
            title = { Text("Supprimer « ${calendar.name} » ?") },
            text = { Text("Les séances de cet agenda disparaîtront de toutes les vues.") },
            confirmButton = {
                TextButton(onClick = {
                    onDelete(calendar.id)
                    deleting = null
                }) { Text("Supprimer") }
            },
            dismissButton = { TextButton(onClick = { deleting = null }) { Text("Annuler") } },
        )
    }
}

@Composable
private fun OverviewCard(state: UiState, onOpen: () -> Unit) {
    Card(
        onClick = onOpen,
        modifier = Modifier.fillMaxWidth(),
        colors = CardDefaults.cardColors(
            containerColor = MaterialTheme.colorScheme.primaryContainer,
        ),
    ) {
        Column(Modifier.padding(16.dp)) {
            Text(
                text = "Vue d'ensemble",
                style = MaterialTheme.typography.titleMedium,
                fontWeight = FontWeight.SemiBold,
            )
            Text(
                text = "Tous les agendas visibles, mêlés.",
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onPrimaryContainer.copy(alpha = 0.75f),
            )
        }
    }
}

@Composable
private fun CalendarFolder(
    summary: CalendarSummary,
    isFirst: Boolean,
    isLast: Boolean,
    onOpen: () -> Unit,
    onRename: () -> Unit,
    onColor: () -> Unit,
    onToggle: (Boolean) -> Unit,
    onUp: () -> Unit,
    onDown: () -> Unit,
    onDelete: () -> Unit,
) {
    val calendar = summary.calendar
    var menu by remember { mutableStateOf(false) }

    Card(onClick = onOpen, modifier = Modifier.fillMaxWidth()) {
        Column(Modifier.padding(16.dp)) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Box(
                    Modifier
                        .size(14.dp)
                        .clip(CircleShape)
                        .background(Color(calendar.color.toInt())),
                )
                Spacer(Modifier.width(12.dp))
                Column(Modifier.weight(1f)) {
                    Text(
                        text = calendar.name,
                        style = MaterialTheme.typography.titleMedium,
                        fontWeight = FontWeight.SemiBold,
                        color = if (calendar.visible) {
                            MaterialTheme.colorScheme.onSurface
                        } else {
                            MaterialTheme.colorScheme.onSurface.copy(alpha = 0.45f)
                        },
                    )
                    Text(
                        text = subtitle(summary),
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurface.copy(alpha = 0.6f),
                    )
                }

                Box {
                    IconButton(onClick = { menu = true }) {
                        Icon(Icons.Outlined.MoreVert, contentDescription = "Options de l'agenda")
                    }
                    DropdownMenu(expanded = menu, onDismissRequest = { menu = false }) {
                        DropdownMenuItem(
                            text = { Text(if (calendar.visible) "Masquer" else "Afficher") },
                            onClick = {
                                onToggle(!calendar.visible)
                                menu = false
                            },
                        )
                        DropdownMenuItem(
                            text = { Text("Renommer") },
                            onClick = {
                                onRename()
                                menu = false
                            },
                        )
                        DropdownMenuItem(
                            text = { Text("Couleur") },
                            onClick = {
                                onColor()
                                menu = false
                            },
                        )
                        if (!isFirst) {
                            DropdownMenuItem(
                                text = { Text("Monter") },
                                leadingIcon = {
                                    Icon(Icons.Outlined.KeyboardArrowUp, contentDescription = null)
                                },
                                onClick = {
                                    onUp()
                                    menu = false
                                },
                            )
                        }
                        if (!isLast) {
                            DropdownMenuItem(
                                text = { Text("Descendre") },
                                leadingIcon = {
                                    Icon(Icons.Outlined.KeyboardArrowDown, contentDescription = null)
                                },
                                onClick = {
                                    onDown()
                                    menu = false
                                },
                            )
                        }
                        DropdownMenuItem(
                            text = { Text("Supprimer") },
                            onClick = {
                                onDelete()
                                menu = false
                            },
                        )
                    }
                }
            }

            summary.next?.let { next ->
                Spacer(Modifier.height(8.dp))
                Text(
                    text = "Prochain : ${next.title}",
                    style = MaterialTheme.typography.bodyMedium,
                    maxLines = 1,
                )
                Text(
                    text = "${next.startUtc.toLocalDate().shortLabel()} · ${next.timeRange()}",
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurface.copy(alpha = 0.6f),
                )
            }

            if (summary.conflicts > 0u) {
                Spacer(Modifier.height(8.dp))
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Icon(
                        Icons.Outlined.Warning,
                        contentDescription = null,
                        tint = MaterialTheme.colorScheme.error,
                        modifier = Modifier.size(16.dp),
                    )
                    Spacer(Modifier.width(6.dp))
                    Text(
                        text = "${summary.conflicts} chevauchement(s) dans le mois",
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.error,
                    )
                }
            }
        }
    }
}

@Composable
private fun ManagerRow(
    title: String,
    subtitle: String,
    alert: Boolean,
    onClick: () -> Unit,
) {
    Card(onClick = onClick, modifier = Modifier.fillMaxWidth()) {
        Row(
            Modifier.padding(16.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Column(Modifier.weight(1f)) {
                Text(
                    text = title,
                    style = MaterialTheme.typography.titleSmall,
                    fontWeight = FontWeight.SemiBold,
                )
                Text(
                    text = subtitle,
                    style = MaterialTheme.typography.bodySmall,
                    color = if (alert) {
                        MaterialTheme.colorScheme.error
                    } else {
                        MaterialTheme.colorScheme.onSurface.copy(alpha = 0.6f)
                    },
                )
            }
            if (alert) {
                Icon(
                    Icons.Outlined.Warning,
                    contentDescription = null,
                    tint = MaterialTheme.colorScheme.error,
                )
            }
        }
    }
}

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

/** Huit pastilles : assez pour distinguer, trop peu pour hésiter. */
@Composable
fun ColorPickerDialog(
    current: UInt,
    onDismiss: () -> Unit,
    onPick: (UInt) -> Unit,
) {
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text("Couleur") },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
                PALETTE.chunked(4).forEach { row ->
                    Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                        row.forEach { color ->
                            Box(
                                Modifier
                                    .size(44.dp)
                                    .clip(CircleShape)
                                    .background(Color(color.toInt()))
                                    .border(
                                        width = if (color == current) 3.dp else 0.dp,
                                        color = MaterialTheme.colorScheme.onSurface,
                                        shape = CircleShape,
                                    )
                                    .clickable { onPick(color) },
                            )
                        }
                    }
                }
            }
        },
        confirmButton = { TextButton(onClick = onDismiss) { Text("Fermer") } },
    )
}

/** Une pastille de catégorie, telle qu'elle apparaît sur les blocs et les listes. */
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

private fun subtitle(summary: CalendarSummary): String {
    val kind = when (summary.calendar.kind) {
        CalendarKind.ICS_FILE -> "Fichier importé"
        CalendarKind.ICS_URL -> "Abonnement"
        CalendarKind.LOCAL -> "Agenda local"
    }
    val week = when (summary.upcomingWeek) {
        0u -> "rien cette semaine"
        1u -> "1 séance cette semaine"
        else -> "${summary.upcomingWeek} séances cette semaine"
    }
    val hidden = if (summary.calendar.visible) null else "masqué"
    return listOfNotNull(kind, week, hidden).joinToString(" · ")
}
