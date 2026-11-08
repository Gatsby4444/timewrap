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
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.outlined.ArrowBack
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextDecoration
import androidx.compose.ui.unit.dp
import app.timewrap.UiState
import app.timewrap.core.ChangeEntry
import app.timewrap.core.ChangeKind
import app.timewrap.core.ChangeSlot

/**
 * L'historique des changements.
 *
 * Une notification se balaie, se tronque, se perd au redémarrage. Ici, chaque
 * synchronisation qui a trouvé quelque chose garde sa liste, et chaque
 * changement montre l'avant et l'après côte à côte. Les nouveautés depuis la
 * dernière visite sont marquées ; elles cessent de l'être en quittant l'écran.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun HistoryScreen(
    state: UiState,
    onLoad: () -> Unit,
    onSeen: () -> Unit,
    onBack: () -> Unit,
) {
    LaunchedEffect(Unit) { onLoad() }
    val markSeen by rememberUpdatedState(onSeen)
    DisposableEffect(Unit) { onDispose { markSeen() } }

    // Une synchronisation = un groupe, dans l'ordre que le cœur a rendu.
    val groups = state.history.groupBy { it.detectedUtc }.toList()

    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text("Changements") },
                navigationIcon = {
                    IconButton(onClick = onBack) {
                        Icon(Icons.AutoMirrored.Outlined.ArrowBack, contentDescription = "Retour")
                    }
                },
            )
        },
    ) { padding ->
        LazyColumn(
            modifier = Modifier
                .fillMaxSize()
                .padding(padding),
            contentPadding = PaddingValues(16.dp),
            verticalArrangement = Arrangement.spacedBy(10.dp),
        ) {
            if (groups.isEmpty()) {
                item {
                    Spacer(Modifier.height(24.dp))
                    Text(
                        text = "Aucun changement pour l'instant.",
                        style = MaterialTheme.typography.titleMedium,
                        fontWeight = FontWeight.SemiBold,
                    )
                    Text(
                        text = "Chaque fois qu'une synchronisation trouve un cours déplacé, " +
                            "annulé, ajouté ou une salle changée, il apparaît ici, avec " +
                            "l'avant et l'après. Les changements sont gardés quatre mois.",
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onBackground.copy(alpha = 0.6f),
                    )
                }
            }

            groups.forEach { (detected, entries) ->
                item(key = "sync-$detected") {
                    Text(
                        text = syncLabel(detected, entries.size, state.today),
                        style = MaterialTheme.typography.labelLarge,
                        color = MaterialTheme.colorScheme.onBackground.copy(alpha = 0.65f),
                        modifier = Modifier.padding(top = 8.dp),
                    )
                }
                items(entries, key = { it.id }) { ChangeCard(it) }
            }
        }
    }
}

@Composable
private fun ChangeCard(entry: ChangeEntry) {
    val change = entry.change
    val accent = kindColor(change.kind)
    Card(
        modifier = Modifier.fillMaxWidth(),
        colors = if (entry.seen) {
            CardDefaults.cardColors()
        } else {
            CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.secondaryContainer)
        },
    ) {
        Column(Modifier.padding(14.dp)) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Box(
                    Modifier
                        .size(10.dp)
                        .background(accent, CircleShape),
                )
                Spacer(Modifier.width(8.dp))
                Text(
                    text = kindLabel(change.kind),
                    style = MaterialTheme.typography.labelMedium,
                    color = accent,
                    fontWeight = FontWeight.SemiBold,
                    modifier = Modifier.weight(1f),
                )
                if (!entry.seen) {
                    Text(
                        text = "Nouveau",
                        style = MaterialTheme.typography.labelSmall,
                        color = MaterialTheme.colorScheme.primary,
                        fontWeight = FontWeight.Bold,
                    )
                }
            }
            Spacer(Modifier.height(6.dp))
            Text(
                text = change.title,
                style = MaterialTheme.typography.titleMedium,
                fontWeight = FontWeight.SemiBold,
            )
            Spacer(Modifier.height(4.dp))

            val before = change.before
            val after = change.after
            when {
                before != null && after != null && change.kind != ChangeKind.CANCELLED &&
                    change.kind != ChangeKind.RESTORED -> {
                    SlotLine("Avant", before, struck = true, compareTo = after)
                    SlotLine("Après", after, struck = false, compareTo = before)
                }
                after != null -> SlotLine(
                    label = if (change.kind == ChangeKind.CANCELLED) "Prévu" else "Quand",
                    slot = after,
                    struck = change.kind == ChangeKind.CANCELLED,
                )
                before != null -> SlotLine("Était prévu", before, struck = true)
            }
        }
    }
}

/**
 * Une ligne « Avant : mardi 22 sept., 13:30 – 15:00 · C204 ».
 *
 * Ce qui diffère de l'autre ligne passe en gras : c'est ce qu'on cherche du
 * regard, et la phrase complète du cœur reste dans la notification.
 */
@Composable
private fun SlotLine(label: String, slot: ChangeSlot, struck: Boolean, compareTo: ChangeSlot? = null) {
    val dateChanged = compareTo != null &&
        (slot.startUtc.toLocalDate() != compareTo.startUtc.toLocalDate())
    val timeChanged = compareTo != null &&
        (slot.startUtc.formatTime() != compareTo.startUtc.formatTime() ||
            slot.endUtc.formatTime() != compareTo.endUtc.formatTime())
    val roomChanged = compareTo != null && slot.location != compareTo.location

    val muted = MaterialTheme.colorScheme.onSurface.copy(alpha = 0.6f)
    val decoration = if (struck) TextDecoration.LineThrough else null
    Row(Modifier.padding(top = 2.dp)) {
        Text(
            text = "$label : ",
            style = MaterialTheme.typography.bodyMedium,
            color = muted,
        )
        Column {
            Text(
                text = slot.startUtc.toLocalDate().shortLabel() + ", " +
                    "${slot.startUtc.formatTime()} – ${slot.endUtc.formatTime()}",
                style = MaterialTheme.typography.bodyMedium,
                fontWeight = if (dateChanged || timeChanged) FontWeight.Bold else FontWeight.Normal,
                textDecoration = decoration,
                color = if (struck) muted else MaterialTheme.colorScheme.onSurface,
            )
            if (slot.location.isNotBlank()) {
                Text(
                    text = slot.location,
                    style = MaterialTheme.typography.bodySmall,
                    fontWeight = if (roomChanged) FontWeight.Bold else FontWeight.Normal,
                    textDecoration = decoration,
                    color = muted,
                )
            }
        }
    }
}

private fun kindLabel(kind: ChangeKind): String = when (kind) {
    ChangeKind.ADDED -> "Ajouté"
    ChangeKind.REMOVED -> "Retiré"
    ChangeKind.MOVED -> "Horaire changé"
    ChangeKind.CANCELLED -> "Annulé"
    ChangeKind.RESTORED -> "Rétabli"
    ChangeKind.ROOM -> "Salle changée"
}

@Composable
private fun kindColor(kind: ChangeKind): Color = when (kind) {
    ChangeKind.CANCELLED, ChangeKind.REMOVED -> MaterialTheme.colorScheme.error
    ChangeKind.ADDED, ChangeKind.RESTORED -> Color(0xFF2E9E7A)
    ChangeKind.MOVED, ChangeKind.ROOM -> MaterialTheme.colorScheme.primary
}

/** « Synchronisation du lun. 7 nov. à 14:02 · 3 changements » */
private fun syncLabel(detectedUtc: Long, count: Int, today: Long): String {
    val day = detectedUtc.toLocalDate()
    val dayLabel = when (day.toEpochDay()) {
        today -> "Aujourd'hui"
        today - 1 -> "Hier"
        else -> day.shortLabel().replaceFirstChar { it.uppercase() }
    }
    val countLabel = if (count == 1) "1 changement" else "$count changements"
    return "$dayLabel à ${detectedUtc.formatTime()} · $countLabel"
}
