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
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.outlined.List
import androidx.compose.material.icons.outlined.Add
import androidx.compose.material.icons.outlined.DateRange
import androidx.compose.material.icons.outlined.Home
import androidx.compose.material.icons.outlined.Menu
import androidx.compose.material3.AssistChip
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
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.lifecycle.viewmodel.compose.viewModel
import app.timewrap.core.EventDraft
import app.timewrap.core.EventOrigin
import app.timewrap.core.Occurrence
import app.timewrap.ui.CategoryChip
import app.timewrap.ui.CategoryPicker
import app.timewrap.ui.ConflictResolutionDialog
import app.timewrap.ui.ConflictsScreen
import app.timewrap.ui.DayScreen
import app.timewrap.ui.EventEditor
import app.timewrap.ui.HomeScreen
import app.timewrap.ui.NowScreen
import app.timewrap.ui.RulesScreen
import app.timewrap.ui.SectionLabel
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

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()
        pendingImport = intent.extractIcsUri()

        val core = (application as TimewrapApp).core

        setContent {
            TimewrapTheme {
                Surface(
                    modifier = Modifier.fillMaxSize(),
                    color = MaterialTheme.colorScheme.background,
                ) {
                    val model: AppViewModel = viewModel(factory = AppViewModel.Factory(core))
                    TimewrapRoot(
                        model = model,
                        pendingImport = pendingImport,
                        onPendingHandled = { pendingImport = null },
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
    }

    /** Lit le contenu d'un `.ics` et devine un nom d'agenda présentable. */
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

private enum class Tab(val label: String, val icon: ImageVector) {
    Home("Agendas", Icons.Outlined.Menu),
    Now("Maintenant", Icons.Outlined.Home),
    Day("Jour", Icons.AutoMirrored.Outlined.List),
    Week("Semaine", Icons.Outlined.DateRange),
}

/**
 * Ce qui se superpose aux vues : les deux gestionnaires et l'éditeur.
 *
 * Un écran plein plutôt qu'une destination de navigation, parce qu'il n'y a
 * jamais qu'un seul niveau : on ouvre, on agit, on revient.
 */
private sealed interface Overlay {
    data object Conflicts : Overlay
    data object Rules : Overlay
    data class Editor(val draft: EventDraft) : Overlay
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun TimewrapRoot(
    model: AppViewModel,
    pendingImport: Uri?,
    onPendingHandled: () -> Unit,
    readIcs: (Uri) -> Pair<String, String>?,
) {
    val state by model.state.collectAsState()
    val liveConflicts by model.draftConflicts.collectAsState()
    var tab by remember { mutableStateOf(Tab.Home) }
    var overlay by remember { mutableStateOf<Overlay?>(null) }
    var detail by remember { mutableStateOf<Occurrence?>(null) }
    val snackbar = remember { SnackbarHostState() }

    val picker = rememberLauncherForActivityResult(
        ActivityResultContracts.OpenDocument(),
    ) { uri ->
        uri?.let { chosen ->
            readIcs(chosen)?.let { (name, text) ->
                model.importIcs(name, chosen.toString(), text)
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
            readIcs(uri)?.let { (name, text) -> model.importIcs(name, uri.toString(), text) }
            onPendingHandled()
        }
    }

    // « Il reste 45 min » se périme en une minute : on rafraîchit doucement,
    // sans réveiller l'appareil ni recharger toute la base.
    LaunchedEffect(Unit) {
        while (true) {
            delay(30_000)
            model.refresh()
        }
    }

    LaunchedEffect(state.message) {
        state.message?.let {
            snackbar.showSnackbar(it)
            model.dismissMessage()
        }
    }

    BackHandler(enabled = overlay != null || tab != Tab.Home) {
        if (overlay != null) overlay = null else tab = Tab.Home
    }

    Scaffold(
        snackbarHost = { SnackbarHost(snackbar) },
        floatingActionButton = {
            val target = state.defaultTarget
            if (tab != Tab.Home && target != null) {
                FloatingActionButton(
                    onClick = {
                        overlay = Overlay.Editor(newDraft(target.id, state.selectedDay))
                    },
                ) { Icon(Icons.Outlined.Add, contentDescription = "Nouvel événement") }
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
        Column(
            Modifier
                .fillMaxSize()
                .padding(padding),
        ) {
            if (tab != Tab.Home) {
                ScopeBanner(state, onClear = { model.openCalendar(null) })
            }
            Box(Modifier.fillMaxSize()) {
                when (tab) {
                    Tab.Home -> HomeScreen(
                        state = state,
                        onOpen = { calendarId ->
                            model.openCalendar(calendarId)
                            tab = Tab.Now
                        },
                        onImport = { openPicker() },
                        onCreate = model::createCalendar,
                        onRename = model::rename,
                        onColor = model::setCalendarColor,
                        onToggle = model::setVisible,
                        onMove = model::moveCalendar,
                        onDelete = model::delete,
                        onConflicts = { overlay = Overlay.Conflicts },
                        onRules = { overlay = Overlay.Rules },
                    )

                    Tab.Now -> NowScreen(
                        state,
                        onImport = { tab = Tab.Home },
                        onSelect = { detail = it },
                    )

                    Tab.Day -> DayScreen(
                        state,
                        onSelectDay = model::selectDay,
                        onSelect = { detail = it },
                    )

                    Tab.Week -> WeekScreen(
                        state,
                        onSelectWeek = model::selectWeek,
                        onSelect = { detail = it },
                    )
                }
            }
        }
    }

    when (val current = overlay) {
        null -> Unit

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
                onBack = { overlay = null },
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

    detail?.let { occurrence ->
        ModalBottomSheet(
            onDismissRequest = { detail = null },
            sheetState = rememberModalBottomSheetState(),
        ) {
            OccurrenceDetail(
                occurrence = occurrence,
                state = state,
                onEdit = {
                    detail = null
                    overlay = Overlay.Editor(draftOf(occurrence))
                },
                onMute = {
                    detail = null
                    model.setMuted(occurrence.id, true)
                },
                onCategory = { model.setOccurrenceCategory(occurrence.id, it) },
            )
        }
    }
}

/** Le rappel discret de l'agenda ouvert, avec la sortie à portée de pouce. */
@Composable
private fun ScopeBanner(state: UiState, onClear: () -> Unit) {
    val open = state.openCalendar ?: return
    Row(
        Modifier
            .fillMaxWidth()
            .padding(horizontal = 16.dp, vertical = 4.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.SpaceBetween,
    ) {
        AssistChip(
            onClick = onClear,
            label = { Text(open.name) },
            leadingIcon = {
                Box(
                    Modifier
                        .size(10.dp)
                        .clip(CircleShape)
                        .background(Color(open.color.toInt())),
                )
            },
        )
        TextButton(onClick = onClear) { Text("Tout voir") }
    }
}

@Composable
private fun OccurrenceDetail(
    occurrence: Occurrence,
    state: UiState,
    onEdit: () -> Unit,
    onMute: () -> Unit,
    onCategory: (String?) -> Unit,
) {
    Column(
        Modifier
            .fillMaxWidth()
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
            if (occurrence.categoryLabel.isNotBlank()) {
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

        Spacer(Modifier.height(16.dp))
        SectionLabel("Agenda")
        Text(occurrence.calendarName, style = MaterialTheme.typography.bodyMedium)

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
