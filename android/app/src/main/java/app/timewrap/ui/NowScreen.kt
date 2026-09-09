package app.timewrap.ui

import androidx.compose.animation.core.LinearEasing
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.tween
import androidx.compose.foundation.background
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
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Button
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import app.timewrap.UiState
import app.timewrap.core.Occurrence
import java.time.LocalDate

/**
 * L'écran qui doit répondre en une seconde : je suis où, il reste combien de
 * temps, c'est quoi après. Tout le reste vient ensuite.
 */
@Composable
fun NowScreen(
    state: UiState,
    onImport: () -> Unit,
    onSelect: (Occurrence) -> Unit,
) {
    if (state.calendars.isEmpty()) {
        EmptyState(onImport)
        return
    }

    val now = state.now

    LazyColumn(
        modifier = Modifier.fillMaxSize(),
        contentPadding = PaddingValues(16.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        item {
            Text(
                text = LocalDate.now(deviceZone).longLabel(),
                style = MaterialTheme.typography.headlineSmall,
                fontWeight = FontWeight.Bold,
            )
        }

        val current = now?.current
        val next = now?.next

        item {
            if (current != null) {
                CurrentCard(
                    occurrence = current,
                    minutesRemaining = now.minutesRemaining ?: 0,
                    onSelect = onSelect,
                )
            } else {
                IdleCard(next, now?.minutesUntilNext)
            }
        }

        if (current != null && next != null) {
            item { SectionLabel("Ensuite") }
            item {
                NextCard(next, now.minutesUntilNext ?: 0, onSelect)
            }
        }

        val rest = now?.restOfDay.orEmpty().filter { it.id != next?.id }
        if (rest.isNotEmpty()) {
            item { SectionLabel("Le reste de la journée") }
            items(rest, key = { it.id }) { occurrence ->
                OccurrenceRow(occurrence, onSelect)
            }
        }

        if (current == null && next == null) {
            item {
                Text(
                    text = "Plus rien de prévu dans les agendas importés.",
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.onBackground.copy(alpha = 0.7f),
                )
            }
        }
    }
}

@Composable
private fun CurrentCard(
    occurrence: Occurrence,
    minutesRemaining: Long,
    onSelect: (Occurrence) -> Unit,
) {
    val total = occurrence.durationMinutes().coerceAtLeast(1)
    val elapsed = (total - minutesRemaining).coerceIn(0, total)
    val progress by animateFloatAsState(
        targetValue = elapsed.toFloat() / total,
        animationSpec = tween(600, easing = LinearEasing),
        label = "progression",
    )
    val accent = Color(occurrence.color.toInt())

    Card(
        modifier = Modifier
            .fillMaxWidth()
            .clickable { onSelect(occurrence) },
        colors = CardDefaults.cardColors(containerColor = accent.copy(alpha = 0.16f)),
    ) {
        Column(Modifier.padding(18.dp)) {
            Text(
                text = "EN COURS",
                style = MaterialTheme.typography.labelSmall,
                fontWeight = FontWeight.Bold,
                color = accent,
            )
            Spacer(Modifier.height(6.dp))
            Text(
                text = occurrence.title,
                style = MaterialTheme.typography.headlineSmall,
                fontWeight = FontWeight.Bold,
            )
            if (occurrence.location.isNotBlank()) {
                Text(
                    text = occurrence.location,
                    style = MaterialTheme.typography.bodyLarge,
                    color = MaterialTheme.colorScheme.onSurface.copy(alpha = 0.75f),
                )
            }
            Spacer(Modifier.height(14.dp))
            Text(
                text = "Il reste ${formatDuration(minutesRemaining)}",
                style = MaterialTheme.typography.titleMedium,
                fontWeight = FontWeight.SemiBold,
            )
            Spacer(Modifier.height(8.dp))
            LinearProgressIndicator(
                progress = { progress },
                modifier = Modifier
                    .fillMaxWidth()
                    .height(6.dp)
                    .clip(RoundedCornerShape(3.dp)),
                color = accent,
                trackColor = accent.copy(alpha = 0.20f),
            )
            Spacer(Modifier.height(6.dp))
            Text(
                text = occurrence.timeRange(),
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurface.copy(alpha = 0.6f),
            )
        }
    }
}

@Composable
private fun IdleCard(next: Occurrence?, minutesUntilNext: Long?) {
    Card(
        modifier = Modifier.fillMaxWidth(),
        colors = CardDefaults.cardColors(
            containerColor = MaterialTheme.colorScheme.surfaceVariant,
        ),
    ) {
        Column(Modifier.padding(18.dp)) {
            Text(
                text = "Rien en cours",
                style = MaterialTheme.typography.headlineSmall,
                fontWeight = FontWeight.Bold,
            )
            if (next != null) {
                Spacer(Modifier.height(10.dp))
                Text(
                    text = "Prochain cours dans ${formatDuration(minutesUntilNext ?: 0)}",
                    style = MaterialTheme.typography.titleMedium,
                )
                Spacer(Modifier.height(4.dp))
                Text(
                    text = "${next.title} · ${next.timeRange()}",
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.onSurface.copy(alpha = 0.75f),
                )
                if (next.location.isNotBlank()) {
                    Text(
                        text = next.location,
                        style = MaterialTheme.typography.bodyMedium,
                        color = MaterialTheme.colorScheme.onSurface.copy(alpha = 0.6f),
                    )
                }
            }
        }
    }
}

@Composable
private fun NextCard(next: Occurrence, minutesUntil: Long, onSelect: (Occurrence) -> Unit) {
    val accent = Color(next.color.toInt())
    Card(
        modifier = Modifier
            .fillMaxWidth()
            .clickable { onSelect(next) },
    ) {
        Row(
            Modifier.padding(16.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Box(
                Modifier
                    .size(width = 4.dp, height = 42.dp)
                    .clip(RoundedCornerShape(2.dp))
                    .background(accent),
            )
            Spacer(Modifier.width(12.dp))
            Column(Modifier.weight(1f)) {
                Text(
                    text = next.title,
                    style = MaterialTheme.typography.titleMedium,
                    fontWeight = FontWeight.SemiBold,
                    maxLines = 2,
                    overflow = TextOverflow.Ellipsis,
                )
                Text(
                    text = listOf(next.timeRange(), next.location)
                        .filter { it.isNotBlank() }
                        .joinToString(" · "),
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.onSurface.copy(alpha = 0.7f),
                )
            }
            Text(
                text = "dans ${formatDuration(minutesUntil)}",
                style = MaterialTheme.typography.labelLarge,
                color = accent,
            )
        }
    }
}

@Composable
fun OccurrenceRow(occurrence: Occurrence, onSelect: (Occurrence) -> Unit) {
    val accent = Color(occurrence.color.toInt())
    Row(
        Modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(10.dp))
            .clickable { onSelect(occurrence) }
            .padding(vertical = 10.dp, horizontal = 4.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Box(
            Modifier
                .size(10.dp)
                .clip(CircleShape)
                .background(if (occurrence.cancelled) accent.copy(alpha = 0.35f) else accent),
        )
        Spacer(Modifier.width(12.dp))
        Column(Modifier.weight(1f)) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                if (occurrence.categoryLabel.isNotBlank()) {
                    CategoryChip(occurrence.categoryLabel, accent)
                    Spacer(Modifier.width(6.dp))
                }
                Text(
                    text = if (occurrence.cancelled) "${occurrence.title} — annulé" else occurrence.title,
                    style = MaterialTheme.typography.bodyLarge,
                    fontWeight = FontWeight.Medium,
                    maxLines = 2,
                    overflow = TextOverflow.Ellipsis,
                )
            }
            if (occurrence.location.isNotBlank()) {
                Text(
                    text = occurrence.location,
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurface.copy(alpha = 0.6f),
                )
            }
        }
        Text(
            text = occurrence.timeRange(),
            style = MaterialTheme.typography.bodyMedium,
            color = MaterialTheme.colorScheme.onSurface.copy(alpha = 0.7f),
        )
    }
}

@Composable
fun SectionLabel(text: String) {
    Text(
        text = text.uppercase(),
        style = MaterialTheme.typography.labelMedium,
        color = MaterialTheme.colorScheme.onBackground.copy(alpha = 0.55f),
        modifier = Modifier.padding(top = 8.dp),
    )
}

@Composable
private fun EmptyState(onImport: () -> Unit) {
    Column(
        modifier = Modifier
            .fillMaxSize()
            .padding(32.dp),
        verticalArrangement = Arrangement.Center,
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        Text(
            text = "Aucun emploi du temps",
            style = MaterialTheme.typography.headlineSmall,
            fontWeight = FontWeight.Bold,
        )
        Spacer(Modifier.height(10.dp))
        Text(
            text = "Exporte ton emploi du temps depuis l'ENT au format iCalendar, " +
                "puis importe le fichier .ics ici.",
            style = MaterialTheme.typography.bodyMedium,
            color = MaterialTheme.colorScheme.onBackground.copy(alpha = 0.7f),
        )
        Spacer(Modifier.height(20.dp))
        Button(onClick = onImport) {
            Text("Importer un fichier .ics")
        }
    }
}
