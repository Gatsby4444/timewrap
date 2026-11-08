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
import androidx.compose.foundation.pager.PagerState
import androidx.compose.foundation.pager.rememberPagerState
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.rememberUpdatedState
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
import kotlin.math.abs

/**
 * Le jour qui porte la page 0 : un lundi, pour que les pages de semaine
 * tombent aussi sur des lundis.
 *
 * Avant, la page centrale valait « aujourd'hui ». Or aujourd'hui change — à
 * minuit, ou en rouvrant l'application le lendemain — alors que le pager, lui,
 * garde sa page : la même page désignait soudain un autre jour, et l'en-tête
 * (calculé d'après la date retenue) ne correspondait plus à la grille
 * (calculée d'après la page). Une origine fixe supprime la cause : une page
 * désigne toujours le même jour.
 */
private val ORIGIN_DAY: Long = LocalDate.of(2000, 1, 3).toEpochDay()

/** Jusqu'en 2079 : on n'en atteint jamais le bord. */
private const val DAY_PAGES = 80 * 366
private const val WEEK_PAGES = DAY_PAGES / 7

private fun dayToPage(epochDay: Long): Int = (epochDay - ORIGIN_DAY).toInt().coerceIn(0, DAY_PAGES - 1)
private fun pageToDay(page: Int): Long = ORIGIN_DAY + page

private fun weekToPage(weekStart: Long): Int =
    ((weekStart.startOfWeek() - ORIGIN_DAY) / 7).toInt().coerceIn(0, WEEK_PAGES - 1)
private fun pageToWeek(page: Int): Long = ORIGIN_DAY + page * 7L

/** Au-delà, faire défiler page par page prendrait des secondes : on saute. */
private const val ANIMATE_LIMIT = 3

/**
 * Accorde le pager à la date retenue, et la date retenue au pager.
 *
 * Une seule vérité : la date retenue par le modèle. Le pager l'affiche, et ne
 * fait que signaler quand l'utilisateur a glissé jusqu'à une autre page. Un
 * changement venu d'ailleurs — le bouton « Aujourd'hui », un import, un
 * événement enregistré, l'onglet À faire — fait donc toujours tourner les
 * pages ; avant, il changeait l'en-tête et laissait la grille où elle était.
 */
@Composable
private fun SyncPager(pager: PagerState, targetPage: Int, onSettled: (Int) -> Unit) {
    val report by rememberUpdatedState(onSettled)

    LaunchedEffect(targetPage) {
        if (pager.currentPage == targetPage) return@LaunchedEffect
        if (abs(pager.currentPage - targetPage) > ANIMATE_LIMIT) {
            pager.scrollToPage(targetPage)
        } else {
            pager.animateScrollToPage(targetPage)
        }
    }

    LaunchedEffect(pager) {
        snapshotFlow { pager.settledPage }.collect { report(it) }
    }
}

@Composable
fun DayScreen(
    state: UiState,
    onSelectDay: (Long) -> Unit,
    onSelect: (Occurrence) -> Unit,
) {
    val today = state.today
    val target = dayToPage(state.selectedDay)
    val pager = rememberPagerState(initialPage = target, pageCount = { DAY_PAGES })
    SyncPager(pager, target) { page -> onSelectDay(pageToDay(page)) }

    // L'en-tête dit ce que montre la grille, pas ce qui a été demandé : pendant
    // un glissement, les deux diffèrent un instant, et c'est la grille qui a
    // raison.
    val shown = pageToDay(pager.currentPage)

    Column(Modifier.fillMaxSize()) {
        val date = LocalDate.ofEpochDay(shown)
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
                    text = when (shown - today) {
                        0L -> "Aujourd'hui"
                        1L -> "Demain"
                        -1L -> "Hier"
                        else -> date.year.toString()
                    },
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onBackground.copy(alpha = 0.6f),
                )
            }
            if (shown != today) {
                TextButton(onClick = { onSelectDay(today) }) { Text("Aujourd'hui") }
            }
        }
        HorizontalDivider()

        HorizontalPager(state = pager, modifier = Modifier.fillMaxSize()) { page ->
            val day = state.day(pageToDay(page))
            when {
                day == null -> Placeholder("Chargement…")
                day.occurrences.isEmpty() -> Placeholder("Rien ce jour-là.")
                else -> TimeGrid(days = listOf(day), today = today, onClick = onSelect)
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
    val currentWeek = state.today.startOfWeek()
    val target = weekToPage(state.weekStart)
    val pager = rememberPagerState(initialPage = target, pageCount = { WEEK_PAGES })
    SyncPager(pager, target) { page -> onSelectWeek(pageToWeek(page)) }

    val shown = pageToWeek(pager.currentPage)

    Column(Modifier.fillMaxSize()) {
        val start = LocalDate.ofEpochDay(shown)
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
            if (shown != currentWeek) {
                TextButton(onClick = { onSelectWeek(currentWeek) }) { Text("Cette semaine") }
            }
        }

        HorizontalPager(state = pager, modifier = Modifier.fillMaxSize()) { page ->
            val week = state.week(pageToWeek(page))
            if (week == null) {
                Placeholder("Chargement…")
            } else {
                TimeGrid(
                    days = week,
                    today = state.today,
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
