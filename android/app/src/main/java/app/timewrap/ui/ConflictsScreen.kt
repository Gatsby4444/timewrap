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
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Card
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import app.timewrap.PendingSave
import app.timewrap.UiState
import app.timewrap.core.ConflictPair
import app.timewrap.core.EventOrigin
import app.timewrap.core.Occurrence
import app.timewrap.core.Resolution

/**
 * Le gestionnaire de chevauchements.
 *
 * Il ne montre que ce qui se marche dessus, sur les deux mois qui viennent, et
 * offre pour chaque camp la seule action que son origine autorise : supprimer un
 * créneau saisi ici, masquer une séance venue de l'ENT.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun ConflictsScreen(
    state: UiState,
    onLoad: () -> Unit,
    onMute: (String) -> Unit,
    onUnmute: (String) -> Unit,
    onDelete: (String) -> Unit,
    onEdit: (Occurrence) -> Unit,
    onBack: () -> Unit,
) {
    LaunchedEffect(Unit) { onLoad() }

    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text("Chevauchements") },
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
            item {
                Text(
                    text = "Sur les deux mois qui viennent.",
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onBackground.copy(alpha = 0.6f),
                )
            }

            if (state.conflicts.isEmpty()) {
                item {
                    Spacer(Modifier.height(24.dp))
                    Text(
                        text = "Rien ne se chevauche.",
                        style = MaterialTheme.typography.titleMedium,
                        fontWeight = FontWeight.SemiBold,
                    )
                    Text(
                        text = "Les créneaux qui s'enchaînent sans se recouvrir ne comptent pas : " +
                            "finir à 10:00 et commencer à 10:00, c'est un enchaînement, pas un conflit.",
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onBackground.copy(alpha = 0.6f),
                    )
                }
            }

            items(state.conflicts, key = { it.first.id + it.second.id }) { pair ->
                ConflictCard(
                    pair = pair,
                    onMute = onMute,
                    onDelete = onDelete,
                    onEdit = onEdit,
                )
            }

            // Une séance masquée sort de toutes les vues, y compris de la liste
            // ci-dessus : sans ce rappel, « Remplacer » serait sans retour.
            if (state.muted.isNotEmpty()) {
                item {
                    Spacer(Modifier.height(12.dp))
                    SectionLabel("Séances masquées")
                }
                items(state.muted, key = { "muted-${it.id}" }) { occurrence ->
                    MutedCard(occurrence) { onUnmute(occurrence.id) }
                }
            }
        }
    }
}

@Composable
private fun MutedCard(occurrence: Occurrence, onUnmute: () -> Unit) {
    Card(Modifier.fillMaxWidth()) {
        Row(
            Modifier.padding(horizontal = 16.dp, vertical = 10.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Column(Modifier.weight(1f)) {
                Text(
                    text = occurrence.title,
                    style = MaterialTheme.typography.titleSmall,
                    color = MaterialTheme.colorScheme.onSurface.copy(alpha = 0.6f),
                )
                Text(
                    text = "${occurrence.startUtc.toLocalDate().shortLabel()} · " +
                        occurrence.timeRange(),
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurface.copy(alpha = 0.5f),
                )
            }
            TextButton(onClick = onUnmute) { Text("Réafficher") }
        }
    }
}

@Composable
private fun ConflictCard(
    pair: ConflictPair,
    onMute: (String) -> Unit,
    onDelete: (String) -> Unit,
    onEdit: (Occurrence) -> Unit,
) {
    Card(Modifier.fillMaxWidth()) {
        Column(Modifier.padding(16.dp)) {
            Text(
                text = "${pair.first.startUtc.toLocalDate().longLabel()} · " +
                    "${pair.overlapMinutes} min en commun",
                style = MaterialTheme.typography.labelMedium,
                color = MaterialTheme.colorScheme.error,
                fontWeight = FontWeight.SemiBold,
            )
            Spacer(Modifier.height(12.dp))
            ConflictSide(pair.first, onMute, onDelete, onEdit)
            HorizontalDivider(Modifier.padding(vertical = 10.dp))
            ConflictSide(pair.second, onMute, onDelete, onEdit)
        }
    }
}

@Composable
private fun ConflictSide(
    occurrence: Occurrence,
    onMute: (String) -> Unit,
    onDelete: (String) -> Unit,
    onEdit: (Occurrence) -> Unit,
) {
    Column {
        Row(verticalAlignment = Alignment.CenterVertically) {
            Box(
                Modifier
                    .size(10.dp)
                    .clip(CircleShape)
                    .background(Color(occurrence.color.toInt())),
            )
            Spacer(Modifier.width(8.dp))
            Text(
                text = occurrence.title,
                style = MaterialTheme.typography.titleSmall,
                fontWeight = FontWeight.SemiBold,
                modifier = Modifier.weight(1f),
            )
            if (occurrence.categoryLabel.isNotBlank()) {
                CategoryChip(occurrence.categoryLabel, Color(occurrence.color.toInt()))
            }
        }
        Text(
            text = occurrence.timeRange(),
            style = MaterialTheme.typography.bodySmall,
            color = MaterialTheme.colorScheme.onSurface.copy(alpha = 0.6f),
        )
        Row(horizontalArrangement = Arrangement.End, modifier = Modifier.fillMaxWidth()) {
            if (occurrence.origin == EventOrigin.LOCAL) {
                TextButton(onClick = { onEdit(occurrence) }) { Text("Déplacer") }
                TextButton(onClick = { onDelete(occurrence.id) }) { Text("Supprimer") }
            } else {
                TextButton(onClick = { onMute(occurrence.id) }) { Text("Masquer") }
            }
        }
    }
}

/**
 * L'arbitrage, quand le cœur a refusé d'écrire.
 *
 * Quatre issues, dans l'ordre où on les envisage : renoncer, faire place nette,
 * décaler, ou assumer le chevauchement.
 */
@Composable
fun ConflictResolutionDialog(
    pending: PendingSave,
    onResolve: (Resolution) -> Unit,
    onCancel: () -> Unit,
) {
    AlertDialog(
        onDismissRequest = onCancel,
        title = {
            Text(
                when (pending.conflicts.size) {
                    1 -> "Ce créneau en chevauche un autre"
                    else -> "Ce créneau en chevauche ${pending.conflicts.size}"
                },
            )
        },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
                pending.conflicts.take(4).forEach { conflict ->
                    Column {
                        Text(
                            text = conflict.other.title,
                            style = MaterialTheme.typography.bodyMedium,
                            fontWeight = FontWeight.Medium,
                        )
                        Text(
                            text = "${conflict.other.timeRange()} · " +
                                "${conflict.overlapMinutes} min en commun",
                            style = MaterialTheme.typography.bodySmall,
                            color = MaterialTheme.colorScheme.onSurface.copy(alpha = 0.65f),
                        )
                        if (!conflict.otherDeletable) {
                            Text(
                                text = "Séance importée : elle sera masquée, pas supprimée.",
                                style = MaterialTheme.typography.bodySmall,
                                color = MaterialTheme.colorScheme.onSurface.copy(alpha = 0.5f),
                            )
                        }
                    }
                }
                Spacer(Modifier.height(4.dp))
                Column {
                    ResolutionButton(
                        "Remplacer",
                        "Libère le créneau : supprime les séances locales, masque les importées.",
                    ) { onResolve(Resolution.REPLACE) }
                    ResolutionButton(
                        "Décaler après",
                        "Repousse ce créneau juste après le dernier conflit, durée conservée.",
                    ) { onResolve(Resolution.SHIFT_AFTER) }
                    ResolutionButton(
                        "Ignorer",
                        "Enregistre quand même : les deux créneaux coexistent.",
                    ) { onResolve(Resolution.IGNORE) }
                }
            }
        },
        confirmButton = {},
        dismissButton = { TextButton(onClick = onCancel) { Text("Annuler") } },
    )
}

@Composable
private fun ResolutionButton(title: String, explanation: String, onClick: () -> Unit) {
    Card(onClick = onClick, modifier = Modifier.fillMaxWidth()) {
        Column(Modifier.padding(horizontal = 14.dp, vertical = 10.dp)) {
            Text(
                text = title,
                style = MaterialTheme.typography.titleSmall,
                fontWeight = FontWeight.SemiBold,
            )
            Text(
                text = explanation,
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurface.copy(alpha = 0.65f),
            )
        }
    }
}
