package app.timewrap

import android.content.Intent
import android.net.Uri
import android.os.Bundle
import android.provider.OpenableColumns
import androidx.activity.ComponentActivity
import androidx.activity.compose.BackHandler
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Button
import androidx.compose.material3.Checkbox
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.ui.text.style.TextDecoration
import app.timewrap.ui.ExamBadge
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.outlined.List
import androidx.compose.material.icons.outlined.Add
import androidx.compose.material.icons.outlined.Check
import androidx.compose.material.icons.outlined.DateRange
import androidx.compose.material.icons.outlined.Home
import androidx.compose.material.icons.outlined.Settings
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.FloatingActionButton
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.NavigationBar
import androidx.compose.material3.NavigationBarItem
import androidx.compose.material3.Scaffold
import androidx.compose.material3.SnackbarHost
import androidx.compose.material3.SnackbarHostState
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.rememberModalBottomSheetState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.compose.LocalLifecycleOwner
import androidx.lifecycle.repeatOnLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import app.timewrap.core.EventDraft
import app.timewrap.core.EventOrigin
import app.timewrap.core.Occurrence
import app.timewrap.core.PropertyKey
import app.timewrap.core.Task
import app.timewrap.notify.Notifications
import app.timewrap.ui.CategoryChip
import app.timewrap.ui.CategoryPicker
import app.timewrap.ui.ColorsScreen
import app.timewrap.ui.ConflictResolutionDialog
import app.timewrap.ui.ConflictsScreen
import app.timewrap.ui.HistoryScreen
import app.timewrap.ui.DayScreen
import app.timewrap.ui.EventEditor
import app.timewrap.ui.NowScreen
import app.timewrap.ui.PropertyValuesScreen
import app.timewrap.ui.ReplaceDialog
import app.timewrap.ui.RulesScreen
import app.timewrap.ui.SectionLabel
import app.timewrap.ui.SettingsScreen
import app.timewrap.ui.TaskEditorDialog
import app.timewrap.ui.TasksScreen
import app.timewrap.ui.WeekScreen
import app.timewrap.ui.draftOf
import app.timewrap.ui.longLabel
import app.timewrap.ui.newDraft
import app.timewrap.ui.theme.TimewrapTheme
import app.timewrap.ui.timeRange
import app.timewrap.ui.toLocalDate
import kotlinx.coroutines.delay

class MainActivity : ComponentActivity() {

    /** Fichier `.ics` ouvert ou partagé depuis une autre application. */
    private var pendingImport by mutableStateOf<Uri?>(null)

    /** Vrai quand l'activité a été ouverte depuis une notification de changement. */
    private var openHistory by mutableStateOf(false)

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()
        pendingImport = intent.extractIcsUri()
        openHistory = intent.wantsHistory()
        Notifications.ensureChannels(this)

        val core = (application as TimewrapApp).core

        setContent {
            TimewrapTheme {
                Surface(
                    modifier = Modifier.fillMaxSize(),
                    color = MaterialTheme.colorScheme.background,
                ) {
                    val model: AppViewModel =
                        viewModel(factory = AppViewModel.Factory(core, this))
                    TimewrapRoot(
                        model = model,
                        pendingImport = pendingImport,
                        onPendingHandled = { pendingImport = null },
                        openHistory = openHistory,
                        onHistoryOpened = { openHistory = false },
                        readIcs = ::readIcs,
                    )
                }
            }
        }
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        setIntent(intent)
        pendingImport = intent.extractIcsUri()
        openHistory = intent.wantsHistory()
    }

    /** Lit le contenu d'un `.ics` et devine un nom présentable. */
    private fun readIcs(uri: Uri): Pair<String, String>? = runCatching {
        val text = contentResolver.openInputStream(uri)?.bufferedReader()?.use { it.readText() }
            ?: return null
        val name = contentResolver
            .query(uri, arrayOf(OpenableColumns.DISPLAY_NAME), null, null, null)
            ?.use { cursor -> if (cursor.moveToFirst()) cursor.getString(0) else null }
            ?: uri.lastPathSegment
            ?: "Emploi du temps"
        name.substringBeforeLast('.').ifBlank { "Emploi du temps" } to text
    }.getOrNull()
}

/** Un `.ics` peut arriver par ouverture directe ou par partage. */
private fun Intent.extractIcsUri(): Uri? = when (action) {
    Intent.ACTION_VIEW -> data
    Intent.ACTION_SEND -> @Suppress("DEPRECATION") getParcelableExtra(Intent.EXTRA_STREAM)
    else -> null
}

private fun Intent.wantsHistory(): Boolean =
    getStringExtra(Notifications.EXTRA_OPEN) == Notifications.OPEN_HISTORY

private enum class Tab(val label: String, val icon: ImageVector) {
    Now("Maintenant", Icons.Outlined.Home),
    Day("Jour", Icons.AutoMirrored.Outlined.List),
    Week("Semaine", Icons.Outlined.DateRange),
    Tasks("À faire", Icons.Outlined.Check),
    Settings("Réglages", Icons.Outlined.Settings),
}

/**
 * Ce qui se superpose aux vues.
 *
 * Un écran plein plutôt qu'une destination de navigation, parce qu'il n'y a
 * jamais qu'un ou deux niveaux : on ouvre, on agit, on revient.
 */
private sealed interface Overlay {
    data object Conflicts : Overlay
    data object Rules : Overlay
    data object Colors : Overlay
    data object History : Overlay
    data class PropertyValues(val key: PropertyKey) : Overlay
    data class Editor(val draft: EventDraft) : Overlay
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun TimewrapRoot(
    model: AppViewModel,
    pendingImport: Uri?,
    onPendingHandled: () -> Unit,
    openHistory: Boolean,
    onHistoryOpened: () -> Unit,
    readIcs: (Uri) -> Pair<String, String>?,
) {
    val state by model.state.collectAsState()
    val liveConflicts by model.draftConflicts.collectAsState()
    var tab by remember { mutableStateOf(Tab.Now) }
    var overlay by remember { mutableStateOf<Overlay?>(null) }
    var editingTask by remember { mutableStateOf<Task?>(null) }
    val snackbar = remember { SnackbarHostState() }

    val picker = rememberLauncherForActivityResult(
        ActivityResultContracts.OpenDocument(),
    ) { uri ->
        uri?.let { chosen ->
            readIcs(chosen)?.let { (name, text) ->
                model.importFile(name, chosen.toString(), text)
            }
        }
    }
    val openPicker = {
        // Beaucoup d'ENT servent le fichier en `application/octet-stream` ;
        // filtrer sur `text/calendar` seul le rendrait invisible.
        picker.launch(arrayOf("text/calendar", "application/octet-stream", "*/*"))
    }

    LaunchedEffect(pendingImport) {
        pendingImport?.let { uri ->
            readIcs(uri)?.let { (name, text) -> model.importFile(name, uri.toString(), text) }
            onPendingHandled()
        }
    }

    LaunchedEffect(openHistory) {
        if (openHistory) {
            overlay = Overlay.History
            onHistoryOpened()
        }
    }

    // « Il reste 45 min » se périme en une minute : on bat doucement, et
    // seulement quand l'application est à l'écran. Le premier battement tombe
    // au retour au premier plan — c'est lui qui voit que le jour a changé
    // pendant la nuit, ou qu'une synchronisation est passée entre-temps.
    val lifecycle = LocalLifecycleOwner.current.lifecycle
    LaunchedEffect(lifecycle) {
        lifecycle.repeatOnLifecycle(Lifecycle.State.STARTED) {
            while (true) {
                model.tick()
                delay(30_000)
            }
        }
    }

    LaunchedEffect(state.message) {
        state.message?.let {
            snackbar.showSnackbar(it)
            model.dismissMessage()
        }
    }

    BackHandler(enabled = overlay != null || tab != Tab.Now) {
        when {
            overlay is Overlay.PropertyValues -> overlay = Overlay.Colors
            overlay != null -> overlay = null
            else -> tab = Tab.Now
        }
    }

    Scaffold(
        snackbarHost = { SnackbarHost(snackbar) },
        floatingActionButton = {
            if (tab == Tab.Day || tab == Tab.Week) {
                if (state.hasTimetable) {
                    FloatingActionButton(
                        onClick = { overlay = Overlay.Editor(newDraft(state.selectedDay)) },
                    ) { Icon(Icons.Outlined.Add, contentDescription = "Nouvel événement") }
                }
            }
        },
        bottomBar = {
            NavigationBar {
                Tab.entries.forEach { entry ->
                    NavigationBarItem(
                        selected = tab == entry,
                        onClick = { tab = entry },
                        icon = { Icon(entry.icon, contentDescription = entry.label) },
                        label = { Text(entry.label) },
                    )
                }
            }
        },
    ) { padding ->
        Box(
            Modifier
                .fillMaxSize()
                .padding(padding),
        ) {
            when (tab) {
                Tab.Now -> NowScreen(
                    state = state,
                    onImport = { tab = Tab.Settings },
                    onOpenHistory = { overlay = Overlay.History },
                    onSelect = model::openCourse,
                    onOpenTasks = { tab = Tab.Tasks },
                )

                Tab.Day -> DayScreen(
                    state,
                    onSelectDay = model::selectDay,
                    onSelect = model::openCourse,
                )

                Tab.Week -> WeekScreen(
                    state,
                    onSelectWeek = model::selectWeek,
                    onSelect = model::openCourse,
                )

                Tab.Tasks -> TasksScreen(
                    state = state,
                    onSelectDay = model::selectDay,
                    onAdd = model::addTask,
                    onToggle = model::setTaskDone,
                    onPostpone = model::postponeTask,
                    onEdit = { editingTask = it },
                    onDelete = model::deleteTask,
                )

                Tab.Settings -> SettingsScreen(
                    state = state,
                    onImportFile = { openPicker() },
                    onSubscribe = model::subscribe,
                    onSyncNow = model::syncNow,
                    onSettings = model::updateSettings,
                    onRename = model::renameTimetable,
                    onColor = model::setTimetableColor,
                    onColors = { overlay = Overlay.Colors },
                    onHistory = { overlay = Overlay.History },
                    onConflicts = { overlay = Overlay.Conflicts },
                    onClear = model::clearTimetable,
                )
            }
        }
    }

    when (val current = overlay) {
        null -> Unit

        Overlay.Colors -> Surface(Modifier.fillMaxSize()) {
            ColorsScreen(
                state = state,
                onLoad = model::loadColors,
                onOpenKey = { overlay = Overlay.PropertyValues(it) },
                onRules = { overlay = Overlay.Rules },
                onBack = { overlay = null },
            )
        }

        Overlay.History -> Surface(Modifier.fillMaxSize()) {
            HistoryScreen(
                state = state,
                onLoad = model::loadHistory,
                onSeen = model::markHistorySeen,
                onBack = { overlay = null },
            )
        }

        is Overlay.PropertyValues -> Surface(Modifier.fillMaxSize()) {
            PropertyValuesScreen(
                propertyKey = current.key,
                values = state.propertyValues,
                onLoad = { model.openProperty(current.key.key) },
                onPick = { value, color ->
                    model.setPropertyColor(current.key.key, value, color)
                },
                onClear = { value -> model.clearPropertyColor(current.key.key, value) },
                onAutoColor = { model.autoColorProperty(current.key.key) },
                onStyle = { value, style -> model.setPropertyStyle(current.key.key, value, style) },
                onAutoStyle = { model.autoStyleProperty(current.key.key) },
                onBack = { overlay = Overlay.Colors },
            )
        }

        Overlay.Conflicts -> Surface(Modifier.fillMaxSize()) {
            ConflictsScreen(
                state = state,
                onLoad = model::loadConflicts,
                onMute = { model.setMuted(it, true) },
                onUnmute = { model.setMuted(it, false) },
                onDelete = model::deleteEvent,
                onEdit = { overlay = Overlay.Editor(draftOf(it)) },
                onBack = { overlay = null },
            )
        }

        Overlay.Rules -> Surface(Modifier.fillMaxSize()) {
            RulesScreen(
                state = state,
                onLoad = model::loadRules,
                onAccept = model::acceptSuggestion,
                onSaveRule = model::saveRule,
                onDeleteRule = model::deleteRule,
                onCreateCategory = model::createCategory,
                onUpdateCategory = model::updateCategory,
                onDeleteCategory = model::deleteCategory,
                onBack = { overlay = Overlay.Colors },
            )
        }

        is Overlay.Editor -> Surface(Modifier.fillMaxSize()) {
            EventEditor(
                state = state,
                initial = current.draft,
                liveConflicts = liveConflicts,
                onCheck = model::checkDraft,
                onSave = {
                    model.saveEvent(it)
                    overlay = null
                },
                onDelete = {
                    model.deleteEvent(it)
                    overlay = null
                },
                onDismiss = { overlay = null },
            )
        }
    }

    // Un import arrive de partout — l'onglet des réglages, ou un .ics ouvert
    // depuis une autre application : la question se pose donc ici, au-dessus de
    // la vue courante quelle qu'elle soit.
    state.pendingImport?.let { pending ->
        ReplaceDialog(
            plan = pending.plan,
            onDismiss = model::cancelImport,
            onConfirm = model::confirmImport,
        )
    }

    // L'arbitrage passe au-dessus de tout : le cœur a refusé d'écrire, il faut
    // répondre avant de reprendre quoi que ce soit d'autre.
    state.pendingSave?.let { pending ->
        ConflictResolutionDialog(
            pending = pending,
            onResolve = model::resolvePending,
            onCancel = {
                model.cancelPending()
                overlay = Overlay.Editor(pending.draft)
            },
        )
    }

    editingTask?.let { task ->
        TaskEditorDialog(
            task = task,
            onDismiss = { editingTask = null },
            onConfirm = { title, notes ->
                model.updateTask(task.id, title, notes)
                editingTask = null
            },
        )
    }

    state.course?.let { sheet ->
        val occurrence = sheet.occurrence
        ModalBottomSheet(
            onDismissRequest = model::closeCourse,
            sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = true),
        ) {
            OccurrenceDetail(
                occurrence = occurrence,
                state = state,
                onEdit = {
                    model.closeCourse()
                    overlay = Overlay.Editor(draftOf(occurrence))
                },
                onMute = {
                    model.closeCourse()
                    model.setMuted(occurrence.id, true)
                },
                onCategory = { model.setOccurrenceCategory(occurrence.id, it) },
                homework = {
                    HomeworkSection(
                        sheet = sheet,
                        onAdd = model::addHomework,
                        onToggle = model::setHomeworkDone,
                    )
                },
            )
        }
    }
}

@Composable
private fun OccurrenceDetail(
    occurrence: Occurrence,
    state: UiState,
    onEdit: () -> Unit,
    onMute: () -> Unit,
    onCategory: (String?) -> Unit,
    homework: @Composable () -> Unit,
) {
    Column(
        Modifier
            .fillMaxWidth()
            .verticalScroll(rememberScrollState())
            .padding(start = 24.dp, end = 24.dp, bottom = 40.dp),
    ) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            Text(
                text = occurrence.title,
                style = MaterialTheme.typography.headlineSmall,
                fontWeight = FontWeight.Bold,
                color = Color(occurrence.color.toInt()),
                modifier = Modifier.weight(1f),
            )
            if (occurrence.exam) {
                ExamBadge(Color(occurrence.color.toInt()))
            } else if (occurrence.categoryLabel.isNotBlank()) {
                CategoryChip(occurrence.categoryLabel, Color(occurrence.color.toInt()))
            }
        }
        Spacer(Modifier.height(10.dp))
        Text(
            text = occurrence.startUtc.toLocalDate().longLabel(),
            style = MaterialTheme.typography.titleMedium,
        )
        Text(
            text = occurrence.timeRange(),
            style = MaterialTheme.typography.bodyLarge,
            color = MaterialTheme.colorScheme.onSurface.copy(alpha = 0.75f),
        )

        if (occurrence.cancelled) {
            Spacer(Modifier.height(8.dp))
            Text(
                text = "Séance annulée",
                style = MaterialTheme.typography.titleSmall,
                color = MaterialTheme.colorScheme.error,
            )
        }

        if (!occurrence.allDay) {
            Spacer(Modifier.height(16.dp))
            homework()
        }

        if (occurrence.location.isNotBlank()) {
            Spacer(Modifier.height(16.dp))
            SectionLabel("Lieu")
            Text(occurrence.location, style = MaterialTheme.typography.bodyLarge)
        }

        if (occurrence.description.isNotBlank()) {
            Spacer(Modifier.height(16.dp))
            SectionLabel("Détails")
            Text(occurrence.description, style = MaterialTheme.typography.bodyMedium)
        }

        if (occurrence.title != occurrence.rawTitle) {
            Spacer(Modifier.height(16.dp))
            SectionLabel("Intitulé d'origine")
            Text(occurrence.rawTitle, style = MaterialTheme.typography.bodySmall)
        }

        if (state.categories.isNotEmpty()) {
            Spacer(Modifier.height(16.dp))
            SectionLabel("Catégorie")
            CategoryPicker(
                categories = state.categories,
                selected = occurrence.categoryId,
                onSelect = onCategory,
            )
        }

        Spacer(Modifier.height(12.dp))
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            if (occurrence.origin == EventOrigin.LOCAL) {
                TextButton(onClick = onEdit) { Text("Modifier") }
            }
            TextButton(onClick = onMute) { Text("Masquer") }
        }
    }
}

/**
 * Les devoirs d'une séance, et la place pour en noter un.
 *
 * Le cas le plus courant en sortant d'un cours : « pour la prochaine fois, faire
 * les exercices 3 à 5 ». Le bouton principal vise donc la prochaine séance du
 * même cours — c'est le cœur qui sait laquelle — et le devoir s'y range, au bon
 * jour. Le second bouton garde la possibilité de le rattacher à celle-ci.
 */
@Composable
private fun HomeworkSection(
    sheet: CourseSheet,
    onAdd: (String, String) -> Unit,
    onToggle: (String, Boolean) -> Unit,
) {
    var text by remember(sheet.occurrence.id) { mutableStateOf("") }
    val next = sheet.next

    SectionLabel("Devoirs")
    if (sheet.homework.isEmpty() && sheet.loaded) {
        Text(
            text = "Rien à faire pour cette séance.",
            style = MaterialTheme.typography.bodySmall,
            color = MaterialTheme.colorScheme.onSurface.copy(alpha = 0.6f),
        )
    }
    sheet.homework.forEach { task -> HomeworkLine(task, onToggle) }

    OutlinedTextField(
        value = text,
        onValueChange = { text = it },
        placeholder = { Text("Noter un devoir…") },
        modifier = Modifier
            .fillMaxWidth()
            .padding(top = 8.dp),
    )
    Row(
        horizontalArrangement = Arrangement.spacedBy(8.dp),
        modifier = Modifier.padding(top = 6.dp),
    ) {
        if (next != null) {
            Button(
                enabled = text.isNotBlank(),
                onClick = {
                    onAdd(text, next.id)
                    text = ""
                },
            ) { Text("Pour le prochain cours") }
        }
        OutlinedButton(
            enabled = text.isNotBlank(),
            onClick = {
                onAdd(text, sheet.occurrence.id)
                text = ""
            },
        ) { Text("Pour cette séance") }
    }

    if (next != null) {
        Text(
            text = "Prochain cours : " + next.startUtc.toLocalDate().longLabel() +
                ", " + next.timeRange() +
                if (next.location.isNotBlank()) " · ${next.location}" else "",
            style = MaterialTheme.typography.bodySmall,
            color = MaterialTheme.colorScheme.onSurface.copy(alpha = 0.6f),
            modifier = Modifier.padding(top = 6.dp),
        )
        sheet.nextHomework.forEach { task -> HomeworkLine(task, onToggle) }
    } else if (sheet.loaded) {
        Text(
            text = "Pas d'autre séance de ce cours à l'emploi du temps.",
            style = MaterialTheme.typography.bodySmall,
            color = MaterialTheme.colorScheme.onSurface.copy(alpha = 0.6f),
            modifier = Modifier.padding(top = 6.dp),
        )
    }
}

@Composable
private fun HomeworkLine(task: Task, onToggle: (String, Boolean) -> Unit) {
    Row(verticalAlignment = Alignment.CenterVertically) {
        Checkbox(checked = task.done, onCheckedChange = { onToggle(task.id, it) })
        Text(
            text = task.title,
            style = MaterialTheme.typography.bodyMedium,
            textDecoration = if (task.done) TextDecoration.LineThrough else null,
            color = if (task.done) {
                MaterialTheme.colorScheme.onSurface.copy(alpha = 0.5f)
            } else {
                MaterialTheme.colorScheme.onSurface
            },
        )
    }
}
