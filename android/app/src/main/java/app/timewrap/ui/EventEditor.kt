package app.timewrap.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.Close
import androidx.compose.material.icons.outlined.Delete
import androidx.compose.material.icons.outlined.Warning
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.DatePicker
import androidx.compose.material3.DatePickerDialog
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.FilterChip
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TimePicker
import androidx.compose.material3.TopAppBar
import androidx.compose.material3.rememberDatePickerState
import androidx.compose.material3.rememberTimePickerState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import app.timewrap.UiState
import app.timewrap.core.Category
import app.timewrap.core.Conflict
import app.timewrap.core.ConflictScope
import app.timewrap.core.EventDraft
import app.timewrap.core.Occurrence
import java.time.LocalDate
import java.time.LocalDateTime
import java.time.LocalTime

/** Durée proposée par défaut à la création d'un créneau. */
private const val DEFAULT_MINUTES = 60L

/**
 * Fabrique un brouillon vide, posé sur le jour consulté.
 *
 * L'heure de départ est arrondie à l'heure suivante : on crée presque toujours
 * un créneau pour « tout à l'heure », rarement pour la minute exacte.
 */
fun newDraft(calendarId: String, epochDay: Long): EventDraft {
    val date = LocalDate.ofEpochDay(epochDay)
    val now = LocalTime.now(deviceZone)
    val start = if (date == LocalDate.now(deviceZone)) {
        date.atTime(now.hour, 0).plusHours(1)
    } else {
        date.atTime(9, 0)
    }
    return EventDraft(
        id = null,
        calendarId = calendarId,
        title = "",
        location = "",
        description = "",
        startUtc = start.toEpochSecondUtc(),
        endUtc = start.plusMinutes(DEFAULT_MINUTES).toEpochSecondUtc(),
        allDay = false,
        categoryId = null,
    )
}

private fun LocalDateTime.toEpochSecondUtc(): Long =
    atZone(deviceZone).toInstant().epochSecond

private fun Long.toLocalDateTime(): LocalDateTime =
    java.time.Instant.ofEpochSecond(this).atZone(deviceZone).toLocalDateTime()

/**
 * L'écran de saisie d'un créneau.
 *
 * Il interroge le moteur de chevauchements à chaque frappe : la question « est-ce
 * que ça tombe sur autre chose ? » doit trouver sa réponse avant d'appuyer sur
 * Enregistrer, pas après.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun EventEditor(
    state: UiState,
    initial: EventDraft,
    liveConflicts: List<Conflict>,
    onCheck: (EventDraft) -> Unit,
    onSave: (EventDraft) -> Unit,
    onDelete: (String) -> Unit,
    onDismiss: () -> Unit,
) {
    var draft by remember { mutableStateOf(initial) }
    var pickingDate by remember { mutableStateOf(false) }
    var pickingStart by remember { mutableStateOf(false) }
    var pickingEnd by remember { mutableStateOf(false) }
    var confirmingDelete by remember { mutableStateOf(false) }

    val start = draft.startUtc.toLocalDateTime()
    val end = draft.endUtc.toLocalDateTime()

    LaunchedEffect(draft.startUtc, draft.endUtc, draft.calendarId, draft.id) {
        onCheck(draft)
    }

    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text(if (draft.id == null) "Nouvel événement" else "Modifier") },
                navigationIcon = {
                    IconButton(onClick = onDismiss) {
                        Icon(Icons.Outlined.Close, contentDescription = "Fermer")
                    }
                },
                actions = {
                    draft.id?.let {
                        IconButton(onClick = { confirmingDelete = true }) {
                            Icon(Icons.Outlined.Delete, contentDescription = "Supprimer")
                        }
                    }
                    TextButton(
                        onClick = { onSave(draft) },
                        enabled = draft.endUtc > draft.startUtc && draft.calendarId.isNotBlank(),
                    ) { Text("Enregistrer") }
                },
            )
        },
    ) { padding ->
        Column(
            Modifier
                .fillMaxSize()
                .padding(padding)
                .verticalScroll(rememberScrollState())
                .padding(horizontal = 16.dp, vertical = 8.dp),
            verticalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            OutlinedTextField(
                value = draft.title,
                onValueChange = { draft = draft.copy(title = it) },
                label = { Text("Titre") },
                singleLine = true,
                modifier = Modifier.fillMaxWidth(),
            )

            SectionLabel("Agenda")
            Row(
                Modifier
                    .fillMaxWidth()
                    .horizontalScroll(rememberScrollState()),
                horizontalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                state.calendars.forEach { calendar ->
                    FilterChip(
                        selected = draft.calendarId == calendar.id,
                        onClick = { draft = draft.copy(calendarId = calendar.id) },
                        label = { Text(calendar.name, maxLines = 1) },
                    )
                }
            }

            SectionLabel("Quand")
            Row(verticalAlignment = Alignment.CenterVertically) {
                Text("Toute la journée", Modifier.weight(1f))
                Switch(
                    checked = draft.allDay,
                    onCheckedChange = { allDay ->
                        draft = if (allDay) {
                            val day = start.toLocalDate().toEpochDay()
                            draft.copy(
                                allDay = true,
                                startUtc = day * 86_400,
                                endUtc = (day + 1) * 86_400,
                            )
                        } else {
                            val day = if (draft.allDay) {
                                LocalDate.ofEpochDay(draft.startUtc / 86_400)
                            } else {
                                start.toLocalDate()
                            }
                            val begin = day.atTime(9, 0)
                            draft.copy(
                                allDay = false,
                                startUtc = begin.toEpochSecondUtc(),
                                endUtc = begin.plusMinutes(DEFAULT_MINUTES).toEpochSecondUtc(),
                            )
                        }
                    },
                )
            }

            val shownDate = if (draft.allDay) {
                LocalDate.ofEpochDay(draft.startUtc / 86_400)
            } else {
                start.toLocalDate()
            }
            FieldButton(label = "Date", value = shownDate.longLabel()) { pickingDate = true }

            if (!draft.allDay) {
                Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                    Column(Modifier.weight(1f)) {
                        FieldButton(label = "Début", value = start.toLocalTime().hhmm()) {
                            pickingStart = true
                        }
                    }
                    Column(Modifier.weight(1f)) {
                        FieldButton(label = "Fin", value = end.toLocalTime().hhmm()) {
                            pickingEnd = true
                        }
                    }
                }
                Text(
                    text = formatDuration((draft.endUtc - draft.startUtc) / 60),
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurface.copy(alpha = 0.6f),
                )
            }

            ConflictBanner(liveConflicts)

            SectionLabel("Classement")
            CategoryPicker(
                categories = state.categories,
                selected = draft.categoryId,
                onSelect = { draft = draft.copy(categoryId = it) },
            )

            OutlinedTextField(
                value = draft.location,
                onValueChange = { draft = draft.copy(location = it) },
                label = { Text("Lieu") },
                singleLine = true,
                modifier = Modifier.fillMaxWidth(),
            )
            OutlinedTextField(
                value = draft.description,
                onValueChange = { draft = draft.copy(description = it) },
                label = { Text("Notes") },
                minLines = 2,
                modifier = Modifier.fillMaxWidth(),
            )
            Spacer(Modifier.height(24.dp))
        }
    }

    if (pickingDate) {
        val picker = rememberDatePickerState(
            initialSelectedDateMillis = shownDayMillis(draft),
        )
        DatePickerDialog(
            onDismissRequest = { pickingDate = false },
            confirmButton = {
                TextButton(onClick = {
                    picker.selectedDateMillis?.let { millis ->
                        // Le sélecteur raisonne en UTC : on n'en garde que la date.
                        val date = java.time.Instant.ofEpochMilli(millis)
                            .atZone(java.time.ZoneOffset.UTC)
                            .toLocalDate()
                        draft = draft.movedTo(date)
                    }
                    pickingDate = false
                }) { Text("Choisir") }
            },
            dismissButton = {
                TextButton(onClick = { pickingDate = false }) { Text("Annuler") }
            },
        ) { DatePicker(state = picker) }
    }

    if (pickingStart) {
        TimePickerDialog(
            title = "Heure de début",
            initial = start.toLocalTime(),
            onDismiss = { pickingStart = false },
            onConfirm = { time ->
                val duration = draft.endUtc - draft.startUtc
                val newStart = start.toLocalDate().atTime(time)
                draft = draft.copy(
                    startUtc = newStart.toEpochSecondUtc(),
                    endUtc = newStart.toEpochSecondUtc() + duration,
                )
                pickingStart = false
            },
        )
    }

    if (pickingEnd) {
        TimePickerDialog(
            title = "Heure de fin",
            initial = end.toLocalTime(),
            onDismiss = { pickingEnd = false },
            onConfirm = { time ->
                var newEnd = start.toLocalDate().atTime(time).toEpochSecondUtc()
                // Une fin avant le début désigne le lendemain, pas une erreur.
                if (newEnd <= draft.startUtc) newEnd += 86_400
                draft = draft.copy(endUtc = newEnd)
                pickingEnd = false
            },
        )
    }

    if (confirmingDelete) {
        val id = draft.id
        AlertDialog(
            onDismissRequest = { confirmingDelete = false },
            title = { Text("Supprimer cet événement ?") },
            confirmButton = {
                TextButton(onClick = {
                    confirmingDelete = false
                    if (id != null) onDelete(id)
                }) { Text("Supprimer") }
            },
            dismissButton = {
                TextButton(onClick = { confirmingDelete = false }) { Text("Annuler") }
            },
        )
    }
}

/** Déplace un brouillon sur une autre date, heures conservées. */
private fun EventDraft.movedTo(date: LocalDate): EventDraft {
    if (allDay) {
        val day = date.toEpochDay()
        return copy(startUtc = day * 86_400, endUtc = (day + 1) * 86_400)
    }
    val duration = endUtc - startUtc
    val start = startUtc.toLocalDateTime()
    val moved = date.atTime(start.toLocalTime()).toEpochSecondUtc()
    return copy(startUtc = moved, endUtc = moved + duration)
}

private fun shownDayMillis(draft: EventDraft): Long {
    val date = if (draft.allDay) {
        LocalDate.ofEpochDay(draft.startUtc / 86_400)
    } else {
        draft.startUtc.toLocalDateTime().toLocalDate()
    }
    return date.toEpochDay() * 86_400_000L
}

private fun LocalTime.hhmm(): String = "%02d:%02d".format(hour, minute)

@Composable
private fun FieldButton(label: String, value: String, onClick: () -> Unit) {
    Card(onClick = onClick, modifier = Modifier.fillMaxWidth()) {
        Column(Modifier.padding(horizontal = 16.dp, vertical = 10.dp)) {
            Text(
                text = label,
                style = MaterialTheme.typography.labelSmall,
                color = MaterialTheme.colorScheme.onSurface.copy(alpha = 0.6f),
            )
            Text(text = value, style = MaterialTheme.typography.titleMedium)
        }
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun TimePickerDialog(
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

@Composable
fun CategoryPicker(
    categories: List<Category>,
    selected: String?,
    onSelect: (String?) -> Unit,
) {
    if (categories.isEmpty()) {
        Text(
            text = "Aucune catégorie pour l'instant. Créez-en depuis les règles visuelles.",
            style = MaterialTheme.typography.bodySmall,
            color = MaterialTheme.colorScheme.onSurface.copy(alpha = 0.6f),
        )
        return
    }
    Row(
        Modifier
            .fillMaxWidth()
            .horizontalScroll(rememberScrollState()),
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        FilterChip(
            selected = selected == null,
            onClick = { onSelect(null) },
            label = { Text("Automatique") },
        )
        categories.forEach { category ->
            FilterChip(
                selected = selected == category.id,
                onClick = { onSelect(category.id) },
                label = { Text(category.name, maxLines = 1) },
            )
        }
    }
}

/** Le bandeau qui prévient pendant la saisie, avant tout enregistrement. */
@Composable
fun ConflictBanner(conflicts: List<Conflict>) {
    if (conflicts.isEmpty()) return
    Card(
        Modifier.fillMaxWidth(),
        colors = CardDefaults.cardColors(
            containerColor = MaterialTheme.colorScheme.errorContainer,
        ),
    ) {
        Column(Modifier.padding(14.dp)) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Icon(
                    Icons.Outlined.Warning,
                    contentDescription = null,
                    tint = MaterialTheme.colorScheme.onErrorContainer,
                    modifier = Modifier.size(18.dp),
                )
                Spacer(Modifier.width(8.dp))
                Text(
                    text = when (conflicts.size) {
                        1 -> "Ce créneau en chevauche un autre"
                        else -> "Ce créneau en chevauche ${conflicts.size}"
                    },
                    style = MaterialTheme.typography.titleSmall,
                    fontWeight = FontWeight.SemiBold,
                    color = MaterialTheme.colorScheme.onErrorContainer,
                )
            }
            conflicts.take(3).forEach { conflict ->
                Spacer(Modifier.height(6.dp))
                ConflictLine(conflict)
            }
        }
    }
}

@Composable
fun ConflictLine(conflict: Conflict) {
    val other = conflict.other
    Column {
        Row(verticalAlignment = Alignment.CenterVertically) {
            Text(
                text = other.title,
                style = MaterialTheme.typography.bodyMedium,
                fontWeight = FontWeight.Medium,
                color = MaterialTheme.colorScheme.onErrorContainer,
                modifier = Modifier.weight(1f, fill = false),
            )
            if (other.categoryLabel.isNotBlank()) {
                Spacer(Modifier.width(6.dp))
                CategoryChip(other.categoryLabel, Color(other.color.toInt()))
            }
        }
        Text(
            text = buildString {
                append(other.timeRange())
                append(" · ")
                append(other.calendarName)
                append(" · ")
                append(
                    when (conflict.scope) {
                        ConflictScope.SAME_CALENDAR -> "même agenda"
                        ConflictScope.CROSS_CALENDAR -> "autre agenda"
                    },
                )
                append(" · ")
                append("${conflict.overlapMinutes} min en commun")
            },
            style = MaterialTheme.typography.bodySmall,
            color = MaterialTheme.colorScheme.onErrorContainer.copy(alpha = 0.8f),
        )
    }
}

/**
 * Le brouillon correspondant à une séance existante.
 *
 * C'est l'intitulé d'origine qu'on remet dans le champ, pas celui qu'une règle
 * affiche : modifier une séance ne doit pas figer son renommage.
 */
fun draftOf(occurrence: Occurrence): EventDraft = EventDraft(
    id = occurrence.id,
    calendarId = occurrence.calendarId,
    title = occurrence.rawTitle,
    location = occurrence.location,
    description = occurrence.description,
    startUtc = occurrence.startUtc,
    endUtc = occurrence.endUtc,
    allDay = occurrence.allDay,
    categoryId = occurrence.categoryId,
)
