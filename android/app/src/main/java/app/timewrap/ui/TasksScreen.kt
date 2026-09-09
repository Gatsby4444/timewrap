package app.timewrap.ui

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
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.Add
import androidx.compose.material.icons.outlined.MoreVert
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.Checkbox
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.style.TextDecoration
import androidx.compose.ui.unit.dp
import app.timewrap.UiState
import app.timewrap.core.Task
import java.time.LocalDate

/**
 * Les choses à faire dans la journée.
 *
 * Sans heure : ce n'est pas un créneau, c'est une intention. Non cochée le soir,
 * une tâche reparaît le lendemain avec son retard affiché — la liste ne se vide
 * pas toute seule, et c'est précisément ce qu'on lui demande.
 */
@Composable
fun TasksScreen(
    state: UiState,
    onSelectDay: (Long) -> Unit,
    onAdd: (String, Long) -> Unit,
    onToggle: (String, Boolean) -> Unit,
    onPostpone: (String, Long) -> Unit,
    onEdit: (Task) -> Unit,
    onDelete: (String) -> Unit,
) {
    val day = state.selectedDay
    val tasks = state.day(day)?.tasks.orEmpty()
    val pending = tasks.filter { !it.done }
    val done = tasks.filter { it.done }

    Column(Modifier.fillMaxSize()) {
        DayStrip(
            selected = day,
            today = state.today,
            onSelect = onSelectDay,
        )

        LazyColumn(
            modifier = Modifier.fillMaxSize(),
            contentPadding = PaddingValues(16.dp),
            verticalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            item { NewTaskField { title -> onAdd(title, day) } }

            if (tasks.isEmpty()) {
                item {
                    Spacer(Modifier.height(24.dp))
                    Text(
                        text = "Rien à faire de noté.",
                        style = MaterialTheme.typography.titleMedium,
                        fontWeight = FontWeight.SemiBold,
                    )
                    Text(
                        text = "Ajoutez ce qui doit être fait dans la journée, sans heure : " +
                            "un devoir à rendre, une démarche, un livre à rapporter. " +
                            "Ce qui n'est pas coché ce soir vous suivra demain.",
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onBackground.copy(alpha = 0.6f),
                    )
                }
            }

            items(pending, key = { it.id }) { task ->
                TaskRow(
                    task = task,
                    onToggle = { onToggle(task.id, it) },
                    onPostpone = { onPostpone(task.id, day + 1) },
                    onEdit = { onEdit(task) },
                    onDelete = { onDelete(task.id) },
                )
            }

            if (done.isNotEmpty()) {
                item {
                    Spacer(Modifier.height(8.dp))
                    SectionLabel("Fait")
                }
                items(done, key = { it.id }) { task ->
                    TaskRow(
                        task = task,
                        onToggle = { onToggle(task.id, it) },
                        onPostpone = { onPostpone(task.id, day + 1) },
                        onEdit = { onEdit(task) },
                        onDelete = { onDelete(task.id) },
                    )
                }
            }
        }
    }
}

/** Sept jours autour de celui qu'on consulte, pour changer de journée d'un doigt. */
@Composable
private fun DayStrip(selected: Long, today: Long, onSelect: (Long) -> Unit) {
    Row(
        Modifier
            .fillMaxWidth()
            .padding(horizontal = 12.dp, vertical = 6.dp),
        horizontalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        (-2L..2L).forEach { offset ->
            val day = selected + offset
            val isSelected = offset == 0L
            val date = LocalDate.ofEpochDay(day)
            Card(
                onClick = { onSelect(day) },
                modifier = Modifier.weight(1f),
                colors = CardDefaults.cardColors(
                    containerColor = when {
                        isSelected -> MaterialTheme.colorScheme.primaryContainer
                        day == today -> MaterialTheme.colorScheme.secondaryContainer
                        else -> MaterialTheme.colorScheme.surfaceVariant
                    },
                ),
            ) {
                Column(
                    Modifier
                        .fillMaxWidth()
                        .padding(vertical = 6.dp),
                    horizontalAlignment = Alignment.CenterHorizontally,
                ) {
                    Text(
                        text = when (day) {
                            today -> "auj."
                            today + 1 -> "dem."
                            else -> date.shortLabel().substringBefore(' ')
                        },
                        style = MaterialTheme.typography.labelSmall,
                    )
                    Text(
                        text = date.dayOfMonth.toString(),
                        style = MaterialTheme.typography.titleMedium,
                        fontWeight = if (isSelected) FontWeight.Bold else FontWeight.Normal,
                    )
                }
            }
        }
    }
}

@Composable
private fun NewTaskField(onAdd: (String) -> Unit) {
    var value by remember { mutableStateOf("") }

    val submit = {
        if (value.isNotBlank()) {
            onAdd(value.trim())
            value = ""
        }
    }

    OutlinedTextField(
        value = value,
        onValueChange = { value = it },
        modifier = Modifier.fillMaxWidth(),
        label = { Text("Ajouter une chose à faire") },
        singleLine = true,
        keyboardOptions = KeyboardOptions(imeAction = ImeAction.Done),
        keyboardActions = KeyboardActions(onDone = { submit() }),
        trailingIcon = {
            IconButton(onClick = submit, enabled = value.isNotBlank()) {
                Icon(Icons.Outlined.Add, contentDescription = "Ajouter")
            }
        },
    )
}

@Composable
private fun TaskRow(
    task: Task,
    onToggle: (Boolean) -> Unit,
    onPostpone: () -> Unit,
    onEdit: () -> Unit,
    onDelete: () -> Unit,
) {
    var menu by remember { mutableStateOf(false) }

    Card(Modifier.fillMaxWidth()) {
        Row(
            Modifier.padding(start = 4.dp, end = 4.dp, top = 2.dp, bottom = 2.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Checkbox(checked = task.done, onCheckedChange = onToggle)
            Column(Modifier.weight(1f)) {
                Text(
                    text = task.title,
                    style = MaterialTheme.typography.bodyLarge,
                    textDecoration = if (task.done) TextDecoration.LineThrough else null,
                    color = if (task.done) {
                        MaterialTheme.colorScheme.onSurface.copy(alpha = 0.5f)
                    } else {
                        MaterialTheme.colorScheme.onSurface
                    },
                )
                lateLabel(task)?.let { late ->
                    Text(
                        text = late,
                        style = MaterialTheme.typography.labelSmall,
                        color = MaterialTheme.colorScheme.error,
                    )
                }
                if (task.notes.isNotBlank()) {
                    Text(
                        text = task.notes,
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurface.copy(alpha = 0.6f),
                    )
                }
            }
            Box {
                IconButton(onClick = { menu = true }) {
                    Icon(Icons.Outlined.MoreVert, contentDescription = "Options")
                }
                DropdownMenu(expanded = menu, onDismissRequest = { menu = false }) {
                    if (!task.done) {
                        DropdownMenuItem(
                            text = { Text("Repousser à demain") },
                            onClick = {
                                onPostpone()
                                menu = false
                            },
                        )
                    }
                    DropdownMenuItem(
                        text = { Text("Modifier") },
                        onClick = {
                            onEdit()
                            menu = false
                        },
                    )
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
    }
}

/**
 * « Prévu hier », « en retard de 3 jours ».
 *
 * Le retard vient du cœur ; l'interface ne fait que choisir les mots, et se
 * tait quand la tâche est à sa place.
 */
private fun lateLabel(task: Task): String? = when {
    task.done || task.daysLate <= 0L -> null
    task.daysLate == 1L -> "Prévu hier, pas fait"
    else -> "En retard de ${task.daysLate} jours"
}

@Composable
fun TaskEditorDialog(
    task: Task,
    onDismiss: () -> Unit,
    onConfirm: (String, String) -> Unit,
) {
    var title by remember { mutableStateOf(task.title) }
    var notes by remember { mutableStateOf(task.notes) }

    androidx.compose.material3.AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text("Modifier") },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
                OutlinedTextField(
                    value = title,
                    onValueChange = { title = it },
                    label = { Text("Intitulé") },
                    singleLine = true,
                )
                OutlinedTextField(
                    value = notes,
                    onValueChange = { notes = it },
                    label = { Text("Notes") },
                    minLines = 2,
                )
            }
        },
        confirmButton = {
            androidx.compose.material3.TextButton(
                onClick = { onConfirm(title, notes) },
                enabled = title.isNotBlank(),
            ) { Text("Enregistrer") }
        },
        dismissButton = {
            androidx.compose.material3.TextButton(onClick = onDismiss) { Text("Annuler") }
        },
    )
}

/** Un rappel discret, en tête de la vue Jour : ce qu'il reste à faire. */
@Composable
fun TaskSummaryCard(state: UiState, onOpen: () -> Unit) {
    val now = state.now ?: return
    if (now.pendingTasks == 0u) return

    Card(
        onClick = onOpen,
        modifier = Modifier.fillMaxWidth(),
        colors = CardDefaults.cardColors(
            containerColor = if (now.lateTasks > 0u) {
                MaterialTheme.colorScheme.errorContainer
            } else {
                MaterialTheme.colorScheme.secondaryContainer
            },
        ),
    ) {
        Row(
            Modifier.padding(14.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Column(Modifier.weight(1f)) {
                Text(
                    text = when (now.pendingTasks) {
                        1u -> "1 chose à faire aujourd'hui"
                        else -> "${now.pendingTasks} choses à faire aujourd'hui"
                    },
                    style = MaterialTheme.typography.titleSmall,
                    fontWeight = FontWeight.SemiBold,
                )
                if (now.lateTasks > 0u) {
                    Text(
                        text = "dont ${now.lateTasks} en retard",
                        style = MaterialTheme.typography.bodySmall,
                    )
                }
            }
            Spacer(Modifier.width(8.dp))
            Text("Voir", style = MaterialTheme.typography.labelLarge)
        }
    }
}
