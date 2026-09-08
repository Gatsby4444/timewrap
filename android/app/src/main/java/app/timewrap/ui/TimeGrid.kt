package app.timewrap.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import app.timewrap.core.DayAgenda
import app.timewrap.core.Occurrence
import java.time.LocalDate
import java.time.LocalTime
import kotlin.math.max
import kotlin.math.min

private val GUTTER_WIDTH = 44.dp
private const val DEFAULT_FIRST_HOUR = 8
private const val DEFAULT_LAST_HOUR = 19

/**
 * La grille horaire commune aux vues Jour et Semaine.
 *
 * Les heures affichées s'ajustent au contenu : inutile de faire défiler un
 * créneau de 6 h du matin quand la première séance est à 8 h. Les journées
 * entières sont sorties de la grille et posées dans un bandeau au-dessus, faute
 * de position horaire à leur donner.
 */
@Composable
fun TimeGrid(
    days: List<DayAgenda>,
    modifier: Modifier = Modifier,
    hourHeight: Dp = 64.dp,
    dayHeader: (@Composable (LocalDate, Boolean) -> Unit)? = null,
    onClick: (Occurrence) -> Unit = {},
) {
    val timed = days.map { day -> day.occurrences.filterNot { it.allDay } }
    val allDay = days.flatMap { day -> day.occurrences.filter { it.allDay } }

    val bounds = remember(days) { hourBounds(timed.flatten()) }
    val (firstHour, lastHour) = bounds
    val hours = lastHour - firstHour

    val scroll = rememberScrollState()
    val today = todayEpochDay()

    // On amène d'emblée le regard sur le début de la journée réelle plutôt que
    // sur une zone vide.
    LaunchedEffect(days.firstOrNull()?.epochDay, firstHour) {
        scroll.animateScrollTo(0)
    }

    Column(modifier) {
        if (dayHeader != null) {
            Row(Modifier.fillMaxWidth()) {
                Box(Modifier.width(GUTTER_WIDTH))
                days.forEach { day ->
                    Box(Modifier.weight(1f), contentAlignment = Alignment.Center) {
                        dayHeader(LocalDate.ofEpochDay(day.epochDay), day.epochDay == today)
                    }
                }
            }
            HorizontalDivider()
        }

        if (allDay.isNotEmpty()) {
            Row(
                Modifier
                    .fillMaxWidth()
                    .horizontalScroll(rememberScrollState())
                    .padding(horizontal = 8.dp, vertical = 6.dp),
                horizontalArrangement = Arrangement.spacedBy(6.dp),
            ) {
                allDay.forEach { AllDayChip(it, onClick) }
            }
            HorizontalDivider()
        }

        Box(
            Modifier
                .fillMaxSize()
                .verticalScroll(scroll),
        ) {
            Column(Modifier.fillMaxWidth()) {
                repeat(hours) { index ->
                    Row(Modifier.height(hourHeight)) {
                        Text(
                            text = "%02d".format(firstHour + index),
                            style = MaterialTheme.typography.labelSmall,
                            color = MaterialTheme.colorScheme.onSurface.copy(alpha = 0.45f),
                            modifier = Modifier
                                .width(GUTTER_WIDTH)
                                .padding(start = 8.dp, top = 2.dp),
                        )
                        Column(Modifier.weight(1f)) {
                            HorizontalDivider(
                                color = MaterialTheme.colorScheme.onSurface.copy(alpha = 0.08f),
                            )
                        }
                    }
                }
            }

            Row(Modifier.fillMaxWidth()) {
                Box(Modifier.width(GUTTER_WIDTH))
                timed.forEach { column ->
                    BoxWithConstraints(
                        Modifier
                            .weight(1f)
                            .height(hourHeight * hours)
                            .padding(horizontal = 2.dp),
                    ) {
                        val columnWidth = maxWidth
                        layout(column).forEach { placed ->
                            OccurrenceBlock(
                                placed = placed,
                                firstHour = firstHour,
                                hourHeight = hourHeight,
                                laneWidth = columnWidth / placed.lanes,
                                compact = timed.size > 1,
                                onClick = onClick,
                            )
                        }
                    }
                }
            }
        }
    }
}

@Composable
private fun AllDayChip(occurrence: Occurrence, onClick: (Occurrence) -> Unit) {
    Box(
        Modifier
            .clip(RoundedCornerShape(6.dp))
            .background(Color(occurrence.color.toInt()).copy(alpha = 0.18f))
            .clickable { onClick(occurrence) }
            .padding(horizontal = 10.dp, vertical = 5.dp),
    ) {
        Text(
            text = occurrence.title,
            style = MaterialTheme.typography.labelMedium,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
        )
    }
}

@Composable
private fun OccurrenceBlock(
    placed: Placed,
    firstHour: Int,
    hourHeight: Dp,
    laneWidth: Dp,
    compact: Boolean,
    onClick: (Occurrence) -> Unit,
) {
    val occurrence = placed.occurrence
    val start = occurrence.startUtc.toLocalTime()
    val minutesFromTop = (start.hour - firstHour) * 60 + start.minute
    val top = hourHeight * (minutesFromTop / 60f)
    val height = maxOf(hourHeight * (occurrence.durationMinutes() / 60f), 26.dp)
    val accent = Color(occurrence.color.toInt())
    val faded = occurrence.cancelled

    Box(
        Modifier
            .width(laneWidth)
            .offset(x = laneWidth * placed.lane, y = top)
            .height(height)
            .padding(vertical = 1.dp)
            .clip(RoundedCornerShape(8.dp))
            .background(accent.copy(alpha = if (faded) 0.10f else 0.20f))
            .clickable { onClick(occurrence) }
            .padding(start = 8.dp, end = 6.dp, top = 3.dp, bottom = 3.dp),
    ) {
        Box(
            Modifier
                .width(3.dp)
                .fillMaxSize()
                .clip(RoundedCornerShape(2.dp))
                .background(if (faded) accent.copy(alpha = 0.4f) else accent),
        )
        Column(Modifier.padding(start = 8.dp)) {
            Text(
                text = occurrence.title,
                style = if (compact) {
                    MaterialTheme.typography.labelSmall
                } else {
                    MaterialTheme.typography.titleSmall
                },
                fontWeight = FontWeight.SemiBold,
                maxLines = if (compact) 3 else 2,
                overflow = TextOverflow.Ellipsis,
                fontSize = if (compact) 10.sp else 14.sp,
            )
            if (!compact && occurrence.location.isNotBlank()) {
                Text(
                    text = occurrence.location,
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurface.copy(alpha = 0.7f),
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
            }
        }
    }
}

/** Une occurrence et la colonne qu'elle occupe quand plusieurs se chevauchent. */
private data class Placed(val occurrence: Occurrence, val lane: Int, val lanes: Int)

/**
 * Répartit les séances qui se chevauchent sur des voies côte à côte, plutôt que
 * de les laisser se recouvrir.
 */
private fun layout(occurrences: List<Occurrence>): List<Placed> {
    if (occurrences.isEmpty()) return emptyList()
    val sorted = occurrences.sortedBy { it.startUtc }
    val laneEnds = mutableListOf<Long>()
    val assigned = sorted.map { occurrence ->
        var lane = laneEnds.indexOfFirst { it <= occurrence.startUtc }
        if (lane < 0) {
            laneEnds.add(occurrence.endUtc)
            lane = laneEnds.lastIndex
        } else {
            laneEnds[lane] = occurrence.endUtc
        }
        occurrence to lane
    }
    val lanes = max(1, laneEnds.size)
    return assigned.map { (occurrence, lane) -> Placed(occurrence, lane, lanes) }
}

/** Bornes horaires de la grille, élargies au contenu du jour. */
private fun hourBounds(occurrences: List<Occurrence>): Pair<Int, Int> {
    if (occurrences.isEmpty()) return DEFAULT_FIRST_HOUR to DEFAULT_LAST_HOUR
    var first = DEFAULT_FIRST_HOUR
    var last = DEFAULT_LAST_HOUR
    occurrences.forEach { occurrence ->
        val start: LocalTime = occurrence.startUtc.toLocalTime()
        val end: LocalTime = occurrence.endUtc.toLocalTime()
        first = min(first, start.hour)
        last = max(last, if (end.minute > 0) end.hour + 1 else end.hour)
    }
    return max(0, first) to min(24, max(last, first + 1))
}
