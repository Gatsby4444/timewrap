package app.timewrap.ui

import android.Manifest
import android.os.Build
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Card
import androidx.compose.material3.FilterChip
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
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
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import app.timewrap.UiState
import app.timewrap.core.Settings
import java.time.Instant
import java.time.LocalTime
import java.time.format.DateTimeFormatter

private val syncFormat = DateTimeFormatter.ofPattern("dd/MM à HH:mm")

/** Délais de rappel proposés : au-delà, on note le cours dans la liste à faire. */
private val LEADS = listOf(5u, 10u, 15u, 30u, 60u)

/** Fréquences de synchronisation proposées. */
private val INTERVALS = listOf(1u, 3u, 6u, 12u, 24u)

/**
 * Les réglages : d'où vient l'emploi du temps, et ce qui doit prévenir.
 *
 * Tout ce qui se règle ici a un effet immédiat et vérifiable — l'abonnement se
 * télécharge à l'enregistrement, les rappels se reposent dès qu'on change le
 * délai. Un réglage dont on ne voit pas l'effet est un réglage qu'on n'ose pas
 * toucher.
 */
@Composable
fun SettingsScreen(
    state: UiState,
    onImportFile: () -> Unit,
    onSubscribe: (String) -> Unit,
    onSyncNow: () -> Unit,
    onSettings: (Settings) -> Unit,
    onRename: (String) -> Unit,
    onColor: (UInt) -> Unit,
    onColors: () -> Unit,
    onConflicts: () -> Unit,
    onClear: () -> Unit,
) {
    val settings = state.settings ?: return
    var subscribing by remember { mutableStateOf(false) }
    var renaming by remember { mutableStateOf(false) }
    var recoloring by remember { mutableStateOf(false) }
    var clearing by remember { mutableStateOf(false) }
    var pickingDigest by remember { mutableStateOf(false) }

    // Sur Android 13 et plus, notifier suppose une permission : on la demande au
    // moment où l'utilisateur active ce qui en a besoin, pas au premier écran.
    val permission = rememberLauncherForActivityResult(
        ActivityResultContracts.RequestPermission(),
    ) { }
    val ensureNotifications = {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
            permission.launch(Manifest.permission.POST_NOTIFICATIONS)
        }
    }

    LazyColumn(
        modifier = Modifier.fillMaxSize(),
        contentPadding = PaddingValues(16.dp),
        verticalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        item { SectionLabel("Emploi du temps") }

        item {
            Card(Modifier.fillMaxWidth()) {
                Column(Modifier.padding(16.dp)) {
                    Text(
                        text = state.timetable?.name ?: "Aucun emploi du temps",
                        style = MaterialTheme.typography.titleMedium,
                        fontWeight = FontWeight.SemiBold,
                    )
                    Text(
                        text = describe(state, settings),
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurface.copy(alpha = 0.6f),
                    )
                    Spacer(Modifier.height(8.dp))
                    Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                        OutlinedButton(onClick = onImportFile) { Text("Fichier .ics") }
                        OutlinedButton(onClick = { subscribing = true }) { Text("Adresse URL") }
                    }
                    if (state.timetable != null) {
                        Row {
                            TextButton(onClick = { renaming = true }) { Text("Renommer") }
                            TextButton(onClick = { recoloring = true }) { Text("Couleur") }
                            TextButton(onClick = { clearing = true }) { Text("Effacer") }
                        }
                    }
                }
            }
        }

        if (settings.sourceUrl.isNotBlank()) {
            item {
                Card(Modifier.fillMaxWidth()) {
                    Column(Modifier.padding(16.dp)) {
                        SwitchRow(
                            title = "Synchronisation automatique",
                            subtitle = "Retélécharge l'abonnement et prévient de ce qui change.",
                            checked = settings.syncEnabled,
                        ) { onSettings(settings.copy(syncEnabled = it)) }

                        if (settings.syncEnabled) {
                            Spacer(Modifier.height(8.dp))
                            Text("Toutes les", style = MaterialTheme.typography.labelMedium)
                            ChipRow(
                                options = INTERVALS,
                                selected = settings.syncIntervalHours,
                                label = { "$it h" },
                            ) { onSettings(settings.copy(syncIntervalHours = it)) }
                        }

                        Spacer(Modifier.height(4.dp))
                        SwitchRow(
                            title = "Prévenir des changements",
                            subtitle = "Cours déplacé, salle changée, séance annulée.",
                            checked = settings.notifyChanges,
                        ) {
                            if (it) ensureNotifications()
                            onSettings(settings.copy(notifyChanges = it))
                        }

                        Spacer(Modifier.height(8.dp))
                        OutlinedButton(onClick = onSyncNow, modifier = Modifier.fillMaxWidth()) {
                            Text("Synchroniser maintenant")
                        }
                    }
                }
            }
        }

        if (state.lastChanges.isNotEmpty()) {
            item {
                Card(Modifier.fillMaxWidth()) {
                    Column(Modifier.padding(16.dp)) {
                        SectionLabel("Derniers changements")
                        state.lastChanges.take(5).forEach { change ->
                            Text(
                                text = "· ${change.summary}",
                                style = MaterialTheme.typography.bodySmall,
                            )
                        }
                    }
                }
            }
        }

        item {
            Spacer(Modifier.height(6.dp))
            SectionLabel("Notifications")
        }

        item {
            Card(Modifier.fillMaxWidth()) {
                Column(Modifier.padding(16.dp)) {
                    SwitchRow(
                        title = "Rappel avant chaque cours",
                        subtitle = "Une notification quelques minutes avant le début.",
                        checked = settings.remindersEnabled,
                    ) {
                        if (it) ensureNotifications()
                        onSettings(settings.copy(remindersEnabled = it))
                    }
                    if (settings.remindersEnabled) {
                        Spacer(Modifier.height(8.dp))
                        Text("Combien à l'avance", style = MaterialTheme.typography.labelMedium)
                        ChipRow(
                            options = LEADS,
                            selected = settings.reminderLeadMinutes,
                            label = { "$it min" },
                        ) { onSettings(settings.copy(reminderLeadMinutes = it)) }
                    }

                    Spacer(Modifier.height(10.dp))
                    SwitchRow(
                        title = "Résumé du matin",
                        subtitle = "Vos cours du jour et ce qu'il reste à faire.",
                        checked = settings.digestEnabled,
                    ) {
                        if (it) ensureNotifications()
                        onSettings(settings.copy(digestEnabled = it))
                    }
                    if (settings.digestEnabled) {
                        TextButton(onClick = { pickingDigest = true }) {
                            Text("À ${formatMinutes(settings.digestMinutes)}")
                        }
                    }
                }
            }
        }

        item {
            Spacer(Modifier.height(6.dp))
            SectionLabel("Apparence et vérifications")
        }

        item {
            Card(onClick = onColors, modifier = Modifier.fillMaxWidth()) {
                Column(Modifier.padding(16.dp)) {
                    Text(
                        text = "Couleurs",
                        style = MaterialTheme.typography.titleSmall,
                        fontWeight = FontWeight.SemiBold,
                    )
                    Text(
                        text = if (state.propertyKeys.isEmpty()) {
                            "Colorier par type de cours, par matière…"
                        } else {
                            state.propertyKeys.joinToString(", ") { it.label }
                        },
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurface.copy(alpha = 0.6f),
                    )
                }
            }
        }

        item {
            Card(onClick = onConflicts, modifier = Modifier.fillMaxWidth()) {
                Column(Modifier.padding(16.dp)) {
                    Text(
                        text = "Chevauchements",
                        style = MaterialTheme.typography.titleSmall,
                        fontWeight = FontWeight.SemiBold,
                    )
                    Text(
                        text = "Les créneaux qui se marchent dessus, et comment les départager.",
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurface.copy(alpha = 0.6f),
                    )
                }
            }
        }

        item {
            Spacer(Modifier.height(10.dp))
            Text(
                text = "Timewrap — tout reste sur l'appareil. Le réseau ne sert qu'à " +
                    "retélécharger l'adresse que vous avez fournie.",
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onBackground.copy(alpha = 0.5f),
            )
        }
    }

    if (subscribing) {
        SubscribeDialog(
            initial = settings.sourceUrl,
            onDismiss = { subscribing = false },
            onConfirm = {
                onSubscribe(it)
                subscribing = false
            },
        )
    }

    if (renaming) {
        TextPromptDialog(
            title = "Renommer",
            hint = "Nom",
            initial = state.timetable?.name.orEmpty(),
            confirm = "Enregistrer",
            onDismiss = { renaming = false },
            onConfirm = {
                if (it.isNotBlank()) onRename(it.trim())
                renaming = false
            },
        )
    }

    if (recoloring) {
        ColorPickerDialog(
            title = "Couleur par défaut",
            current = state.timetable?.color,
            onDismiss = { recoloring = false },
            onPick = {
                onColor(it)
                recoloring = false
            },
        )
    }

    if (pickingDigest) {
        TimePickerDialog(
            title = "Résumé du matin",
            initial = LocalTime.of(
                (settings.digestMinutes / 60u).toInt(),
                (settings.digestMinutes % 60u).toInt(),
            ),
            onDismiss = { pickingDigest = false },
            onConfirm = { time ->
                onSettings(settings.copy(digestMinutes = (time.hour * 60 + time.minute).toUInt()))
                pickingDigest = false
            },
        )
    }

    if (clearing) {
        AlertDialog(
            onDismissRequest = { clearing = false },
            title = { Text("Effacer l'emploi du temps ?") },
            text = {
                Text(
                    "Les séances disparaîtront de toutes les vues. " +
                        "Vos choses à faire, elles, restent.",
                )
            },
            confirmButton = {
                TextButton(onClick = {
                    onClear()
                    clearing = false
                }) { Text("Effacer") }
            },
            dismissButton = { TextButton(onClick = { clearing = false }) { Text("Annuler") } },
        )
    }
}

@Composable
private fun SwitchRow(
    title: String,
    subtitle: String,
    checked: Boolean,
    onChange: (Boolean) -> Unit,
) {
    Row(verticalAlignment = Alignment.CenterVertically) {
        Column(Modifier.weight(1f)) {
            Text(title, style = MaterialTheme.typography.bodyLarge)
            Text(
                text = subtitle,
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurface.copy(alpha = 0.6f),
            )
        }
        Switch(checked = checked, onCheckedChange = onChange)
    }
}

@Composable
private fun ChipRow(
    options: List<UInt>,
    selected: UInt,
    label: (UInt) -> String,
    onPick: (UInt) -> Unit,
) {
    Row(
        Modifier
            .fillMaxWidth()
            .horizontalScroll(rememberScrollState()),
        horizontalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        options.forEach { option ->
            FilterChip(
                selected = option == selected,
                onClick = { onPick(option) },
                label = { Text(label(option)) },
            )
        }
    }
}

@Composable
private fun SubscribeDialog(
    initial: String,
    onDismiss: () -> Unit,
    onConfirm: (String) -> Unit,
) {
    var url by remember { mutableStateOf(initial) }
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text("Abonnement par URL") },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
                Text(
                    text = "Collez l'adresse d'export iCalendar de votre ENT. " +
                        "Les adresses webcal:// fonctionnent aussi.",
                    style = MaterialTheme.typography.bodySmall,
                )
                OutlinedTextField(
                    value = url,
                    onValueChange = { url = it },
                    label = { Text("https://…") },
                    singleLine = true,
                )
            }
        },
        confirmButton = {
            TextButton(
                onClick = { onConfirm(url) },
                enabled = url.isNotBlank(),
            ) { Text("S'abonner") }
        },
        dismissButton = { TextButton(onClick = onDismiss) { Text("Annuler") } },
    )
}

private fun describe(state: UiState, settings: Settings): String {
    val timetable = state.timetable ?: return "Importez un fichier .ics, ou abonnez-vous à une URL."
    val source = if (settings.sourceUrl.isNotBlank()) "abonnement" else "fichier importé"
    val counts = "${timetable.eventCount} cours · ${timetable.occurrenceCount} séances"
    val sync = timetable.lastSync?.let {
        "à jour le " + Instant.ofEpochSecond(it).atZone(deviceZone).format(syncFormat)
    }
    return listOfNotNull(source, counts, sync).joinToString(" · ")
}

private fun formatMinutes(minutes: UInt): String =
    "%02d:%02d".format((minutes / 60u).toInt(), (minutes % 60u).toInt())
