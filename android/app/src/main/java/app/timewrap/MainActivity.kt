package app.timewrap

import android.content.Intent
import android.net.Uri
import android.os.Bundle
import android.provider.OpenableColumns
import androidx.activity.ComponentActivity
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.DateRange
import androidx.compose.material.icons.outlined.Home
import androidx.compose.material.icons.outlined.List
import androidx.compose.material.icons.outlined.Settings
import androidx.compose.material3.ExperimentalMaterial3Api
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
import androidx.compose.material3.rememberModalBottomSheetState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.lifecycle.viewmodel.compose.viewModel
import kotlinx.coroutines.delay
import app.timewrap.core.Occurrence
import app.timewrap.ui.CalendarsScreen
import app.timewrap.ui.DayScreen
import app.timewrap.ui.NowScreen
import app.timewrap.ui.SectionLabel
import app.timewrap.ui.WeekScreen
import app.timewrap.ui.longLabel
import app.timewrap.ui.theme.TimewrapTheme
import app.timewrap.ui.timeRange
import app.timewrap.ui.toLocalDate

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
    Now("Maintenant", Icons.Outlined.Home),
    Day("Jour", Icons.Outlined.List),
    Week("Semaine", Icons.Outlined.DateRange),
    Calendars("Agendas", Icons.Outlined.Settings),
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
    var tab by remember { mutableStateOf(Tab.Now) }
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

    Scaffold(
        snackbarHost = { SnackbarHost(snackbar) },
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
                Tab.Now -> NowScreen(state, onImport = { openPicker() }, onSelect = { detail = it })
                Tab.Day -> DayScreen(state, onSelectDay = model::selectDay, onSelect = { detail = it })
                Tab.Week -> WeekScreen(state, onSelectWeek = model::selectWeek, onSelect = { detail = it })
                Tab.Calendars -> CalendarsScreen(
                    state = state,
                    onImport = { openPicker() },
                    onToggle = model::setVisible,
                    onRename = model::rename,
                    onDelete = model::delete,
                )
            }
        }
    }

    detail?.let { occurrence ->
        ModalBottomSheet(
            onDismissRequest = { detail = null },
            sheetState = rememberModalBottomSheetState(),
        ) {
            OccurrenceDetail(occurrence)
        }
    }
}

@Composable
private fun OccurrenceDetail(occurrence: Occurrence) {
    Column(
        Modifier
            .fillMaxWidth()
            .padding(start = 24.dp, end = 24.dp, bottom = 40.dp),
    ) {
        Text(
            text = occurrence.title,
            style = MaterialTheme.typography.headlineSmall,
            fontWeight = FontWeight.Bold,
            color = Color(occurrence.color.toInt()),
        )
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
    }
}
