package app.timewrap.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.pager.HorizontalPager
import androidx.compose.foundation.pager.rememberPagerState
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.snapshotFlow
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import app.timewrap.UiState
import app.timewrap.core.DayAgenda
import app.timewrap.core.Occurrence
import java.time.LocalDate
import java.time.format.TextStyle
import java.util.Locale

/**
 * Le pager couvre environ dix ans de part et d'autre d'aujourd'hui : assez pour
 * qu'on n'en atteigne jamais le bord, sans rien précharger.
 */
private const val PAGE_COUNT = 7300
private const val PAGE_ORIGIN = PAGE_COUNT / 2

@Composable
fun DayScreen(
    state: UiState,
    onSelectDay: (Long) -> Unit,
    onSelect: (Occurrence) -> Unit,
) {
    val today = todayEpochDay()
    val pager = rememberPagerState(
        initialPage = PAGE_ORIGIN + (state.selectedDay - today).toInt(),
        pageCount = { PAGE_COUNT },
    )

    LaunchedEffect(pager) {
        snapshotFlow { pager.settledPage }.collect { page ->
            onSelectDay(today + (page - PAGE_ORIGIN))
        }
    }

    Column(Modifier.fillMaxSize()) {
        val date = LocalDate.ofEpochDay(state.selectedDay)
        Row(
            Modifier
                .fillMaxWidth()
                .padding(start = 16.dp, end = 8.dp, top = 8.dp, bottom = 4.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Column(Modifier.weight(1f)) {
                Text(
                    text = date.longLabel(),
                    style = MaterialTheme.typography.titleLarge,
                    fontWeight = FontWeight.Bold,
                )
                Text(
                    text = when (state.selectedDay - today) {
                        0L -> "Aujourd'hui"
                        1L -> "Demain"
                        -1L -> "Hier"
                        else -> date.year.toString()
                    },
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onBackground.copy(alpha = 0.6f),
                )
            }
            if (state.selectedDay != today) {
                TextButton(onClick = { onSelectDay(today) }) { Text("Aujourd'hui") }
            }
        }
        HorizontalDivider()

        HorizontalPager(state = pager, modifier = Modifier.fillMaxSize()) { page ->
            val epochDay = today + (page - PAGE_ORIGIN)
            val day = state.day(epochDay)
            when {
                day == null -> Placeholder("Chargement…")
                day.occurrences.isEmpty() -> Placeholder("Rien ce jour-là.")
                else -> TimeGrid(days = listOf(day), onClick = onSelect)
            }
        }
    }
}

@Composable
fun WeekScreen(
    state: UiState,
    onSelectWeek: (Long) -> Unit,
    onSelect: (Occurrence) -> Unit,
) {
    val currentWeek = todayEpochDay().startOfWeek()
    val pager = rememberPagerState(
        initialPage = PAGE_ORIGIN + ((state.weekStart - currentWeek) / 7).toInt(),
        pageCount = { PAGE_COUNT },
    )

    LaunchedEffect(pager) {
        snapshotFlow { pager.settledPage }.collect { page ->
            onSelectWeek(currentWeek + (page - PAGE_ORIGIN) * 7L)
        }
    }

    Column(Modifier.fillMaxSize()) {
        val start = LocalDate.ofEpochDay(state.weekStart)
        val end = start.plusDays(6)
        Row(
            Modifier
                .fillMaxWidth()
                .padding(start = 16.dp, end = 8.dp, top = 8.dp, bottom = 4.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text(
                text = "${start.dayOfMonth} ${start.month.getDisplayName(TextStyle.SHORT, Locale.getDefault())}" +
                    " – ${end.dayOfMonth} ${end.month.getDisplayName(TextStyle.SHORT, Locale.getDefault())}",
                style = MaterialTheme.typography.titleLarge,
                fontWeight = FontWeight.Bold,
                modifier = Modifier.weight(1f),
            )
            if (state.weekStart != currentWeek) {
                TextButton(onClick = { onSelectWeek(currentWeek) }) { Text("Cette semaine") }
            }
        }

        HorizontalPager(state = pager, modifier = Modifier.fillMaxSize()) { page ->
            val weekStart = currentWeek + (page - PAGE_ORIGIN) * 7L
            val week = state.week(weekStart)
            if (week == null) {
                Placeholder("Chargement…")
            } else {
                TimeGrid(
                    days = week,
                    hourHeight = 52.dp,
                    dayHeader = { date, isToday -> DayHeader(date, isToday) },
                    onClick = onSelect,
                )
            }
        }
    }
}

@Composable
private fun DayHeader(date: LocalDate, isToday: Boolean) {
    Column(
        modifier = Modifier.padding(vertical = 6.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        Text(
            text = date.dayOfWeek.getDisplayName(TextStyle.SHORT, Locale.getDefault())
                .take(3)
                .replaceFirstChar { it.uppercase(Locale.getDefault()) },
            style = MaterialTheme.typography.labelSmall,
            color = MaterialTheme.colorScheme.onBackground.copy(alpha = 0.6f),
        )
        Text(
            text = date.dayOfMonth.toString(),
            style = MaterialTheme.typography.titleSmall,
            fontWeight = if (isToday) FontWeight.Bold else FontWeight.Normal,
            color = if (isToday) {
                MaterialTheme.colorScheme.primary
            } else {
                MaterialTheme.colorScheme.onBackground
            },
        )
    }
}

/** Repli quand la journée demandée n'est pas encore chargée, ou qu'elle est vide. */
@Composable
private fun Placeholder(text: String) {
    Column(
        modifier = Modifier
            .fillMaxSize()
            .padding(32.dp),
        verticalArrangement = Arrangement.Center,
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        Text(
            text = text,
            style = MaterialTheme.typography.bodyLarge,
            color = MaterialTheme.colorScheme.onBackground.copy(alpha = 0.55f),
            textAlign = TextAlign.Center,
        )
    }
}

/** Vue liste d'une journée, utilisée quand la grille n'apporte rien. */
@Composable
fun DayList(day: DayAgenda, onSelect: (Occurrence) -> Unit) {
    LazyColumn(
        contentPadding = PaddingValues(16.dp),
        verticalArrangement = Arrangement.spacedBy(4.dp),
    ) {
        items(day.occurrences, key = { it.id }) { OccurrenceRow(it, onSelect) }
    }
}
