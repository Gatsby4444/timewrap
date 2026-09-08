package app.timewrap.ui

import androidx.compose.foundation.background
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
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.Card
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Switch
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
import java.time.Instant
import java.time.format.DateTimeFormatter

private val syncFormat = DateTimeFormatter.ofPattern("dd/MM à HH:mm")

@Composable
fun CalendarsScreen(
    state: UiState,
    onImport: () -> Unit,
    onToggle: (String, Boolean) -> Unit,
    onRename: (String, String) -> Unit,
    onDelete: (String) -> Unit,
) {
    var renaming by remember { mutableStateOf<Calendar?>(null) }
    var deleting by remember { mutableStateOf<Calendar?>(null) }

    LazyColumn(
        modifier = Modifier.fillMaxSize(),
        contentPadding = PaddingValues(16.dp),
        verticalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        item {
            Text(
                text = "Agendas",
                style = MaterialTheme.typography.headlineSmall,
                fontWeight = FontWeight.Bold,
            )
        }

        items(state.calendars, key = { it.id }) { calendar ->
            CalendarCard(
                calendar = calendar,
                onToggle = { onToggle(calendar.id, it) },
                onRename = { renaming = calendar },
                onDelete = { deleting = calendar },
            )
        }

        item {
            Spacer(Modifier.height(4.dp))
            Button(onClick = onImport, modifier = Modifier.fillMaxWidth()) {
                Text("Importer un fichier .ics")
            }
        }

        item {
            Text(
                text = "L'abonnement par URL, qui met l'emploi du temps à jour tout seul, " +
                    "arrive à la prochaine étape.",
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onBackground.copy(alpha = 0.55f),
            )
        }
    }

    renaming?.let { calendar ->
        RenameDialog(
            calendar = calendar,
            onDismiss = { renaming = null },
            onConfirm = { name ->
                onRename(calendar.id, name)
                renaming = null
            },
        )
    }

    deleting?.let { calendar ->
        AlertDialog(
            onDismissRequest = { deleting = null },
            title = { Text("Supprimer « ${calendar.name} » ?") },
            text = { Text("Les séances importées depuis cet agenda disparaîtront de toutes les vues.") },
            confirmButton = {
                TextButton(onClick = {
                    onDelete(calendar.id)
                    deleting = null
                }) { Text("Supprimer") }
            },
            dismissButton = {
                TextButton(onClick = { deleting = null }) { Text("Annuler") }
            },
        )
    }
}

@Composable
private fun CalendarCard(
    calendar: Calendar,
    onToggle: (Boolean) -> Unit,
    onRename: () -> Unit,
    onDelete: () -> Unit,
) {
    Card(Modifier.fillMaxWidth()) {
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
                    )
                    Text(
                        text = subtitle(calendar),
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurface.copy(alpha = 0.6f),
                    )
                }
                Switch(checked = calendar.visible, onCheckedChange = onToggle)
            }
            Row(horizontalArrangement = Arrangement.End, modifier = Modifier.fillMaxWidth()) {
                TextButton(onClick = onRename) { Text("Renommer") }
                TextButton(onClick = onDelete) { Text("Supprimer") }
            }
        }
    }
}

@Composable
private fun RenameDialog(
    calendar: Calendar,
    onDismiss: () -> Unit,
    onConfirm: (String) -> Unit,
) {
    var name by remember { mutableStateOf(calendar.name) }
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text("Renommer l'agenda") },
        text = {
            OutlinedTextField(
                value = name,
                onValueChange = { name = it },
                singleLine = true,
                label = { Text("Nom") },
            )
        },
        confirmButton = {
            TextButton(
                onClick = { onConfirm(name.trim().ifBlank { calendar.name }) },
            ) { Text("Enregistrer") }
        },
        dismissButton = { TextButton(onClick = onDismiss) { Text("Annuler") } },
    )
}

private fun subtitle(calendar: Calendar): String {
    val kind = when (calendar.kind) {
        CalendarKind.ICS_FILE -> "Fichier importé"
        CalendarKind.ICS_URL -> "Abonnement"
        CalendarKind.LOCAL -> "Agenda local"
    }
    val count = "${calendar.eventCount} cours"
    val sync = calendar.lastSync?.let {
        "importé le " + Instant.ofEpochSecond(it).atZone(deviceZone).format(syncFormat)
    }
    return listOfNotNull(kind, count, sync).joinToString(" · ")
}
