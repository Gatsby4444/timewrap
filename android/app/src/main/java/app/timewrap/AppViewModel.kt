package app.timewrap

import androidx.lifecycle.ViewModel
import androidx.lifecycle.ViewModelProvider
import androidx.lifecycle.viewModelScope
import app.timewrap.core.Calendar
import app.timewrap.core.CalendarKind
import app.timewrap.core.DayAgenda
import app.timewrap.core.ImportReport
import app.timewrap.core.NowView
import app.timewrap.core.Timewrap
import app.timewrap.ui.startOfWeek
import app.timewrap.ui.todayEpochDay
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

/** Jours chargés d'avance de part et d'autre de la date consultée. */
private const val PRELOAD_RADIUS = 21L

data class UiState(
    val calendars: List<Calendar> = emptyList(),
    val now: NowView? = null,
    val selectedDay: Long = todayEpochDay(),
    val weekStart: Long = todayEpochDay().startOfWeek(),
    /// Journées déjà chargées, indexées par `epochDay`.
    val days: Map<Long, DayAgenda> = emptyMap(),
    val busy: Boolean = false,
    /// Message éphémère : bilan d'import ou erreur.
    val message: String? = null,
) {
    fun day(epochDay: Long): DayAgenda? = days[epochDay]

    /** Les sept jours d'une semaine, ou `null` tant qu'il en manque un. */
    fun week(start: Long): List<DayAgenda>? {
        val found = (0L until 7L).mapNotNull { days[start + it] }
        return found.takeIf { it.size == 7 }
    }
}

class AppViewModel(private val core: Timewrap) : ViewModel() {

    private val _state = MutableStateFlow(UiState())
    val state: StateFlow<UiState> = _state.asStateFlow()

    /** Bornes déjà en cache, pour ne recharger que ce qui manque. */
    private var loadedFrom: Long = Long.MAX_VALUE
    private var loadedTo: Long = Long.MIN_VALUE

    init {
        refresh()
    }

    /** Recharge ce qui est affiché : agendas, vue « maintenant », jours en cache. */
    fun refresh() = launchCore {
        val snapshot = _state.value
        val calendars = core.calendars()
        val now = core.nowView()
        _state.update { it.copy(calendars = calendars, now = now) }
        invalidateDays()
        loadAround(snapshot.selectedDay)
        loadAround(snapshot.weekStart + 3)
    }

    fun selectDay(epochDay: Long) {
        _state.update { it.copy(selectedDay = epochDay) }
        launchCore { loadAround(epochDay) }
    }

    fun selectWeek(weekStart: Long) {
        _state.update { it.copy(weekStart = weekStart) }
        launchCore { loadAround(weekStart + 3) }
    }

    fun importIcs(name: String, source: String, text: String) = launchCore {
        val report = core.importIcs(name, CalendarKind.ICS_FILE, source, text)
        val today = todayEpochDay()
        _state.update {
            it.copy(
                message = importSummary(name, report),
                selectedDay = today,
                weekStart = today.startOfWeek(),
            )
        }
        reload(today)
    }

    fun setVisible(calendarId: String, visible: Boolean) = launchCore {
        core.setCalendarVisible(calendarId, visible)
        reload(_state.value.selectedDay)
    }

    fun rename(calendarId: String, name: String) = launchCore {
        core.renameCalendar(calendarId, name)
        reload(_state.value.selectedDay)
    }

    fun delete(calendarId: String) = launchCore {
        core.deleteCalendar(calendarId)
        reload(_state.value.selectedDay)
    }

    fun dismissMessage() = _state.update { it.copy(message = null) }

    /** Après une modification du contenu, le cache de jours n'est plus fiable. */
    private suspend fun reload(around: Long) {
        val calendars = core.calendars()
        val now = core.nowView()
        _state.update { it.copy(calendars = calendars, now = now) }
        invalidateDays()
        loadAround(around)
        loadAround(_state.value.weekStart + 3)
    }

    private fun invalidateDays() {
        loadedFrom = Long.MAX_VALUE
        loadedTo = Long.MIN_VALUE
        _state.update { it.copy(days = emptyMap()) }
    }

    /**
     * Charge la fenêtre autour d'un jour, en n'appelant le cœur que pour la
     * partie manquante : balayer jour après jour ne doit pas tout recharger.
     */
    private suspend fun loadAround(epochDay: Long) {
        val wanted = (epochDay - PRELOAD_RADIUS)..(epochDay + PRELOAD_RADIUS)
        val missing = when {
            loadedFrom > loadedTo -> wanted
            wanted.first < loadedFrom && wanted.last > loadedTo -> wanted
            wanted.first < loadedFrom -> wanted.first until loadedFrom
            wanted.last > loadedTo -> (loadedTo + 1)..wanted.last
            else -> return
        }

        val count = (missing.last - missing.first + 1).toInt()
        if (count <= 0) return
        val loaded = core.days(missing.first, count.toUInt())

        _state.update { current -> current.copy(days = current.days + loaded.associateBy { it.epochDay }) }
        loadedFrom = minOf(loadedFrom, missing.first)
        loadedTo = maxOf(loadedTo, missing.last)
    }

    /**
     * Toute traversée vers le cœur passe par ici : hors du fil principal, et
     * une erreur devient un message lisible plutôt qu'un plantage.
     */
    private fun launchCore(block: suspend () -> Unit) {
        viewModelScope.launch {
            _state.update { it.copy(busy = true) }
            try {
                withContext(Dispatchers.IO) { block() }
            } catch (e: Exception) {
                _state.update { it.copy(message = e.message ?: e.toString()) }
            } finally {
                _state.update { it.copy(busy = false) }
            }
        }
    }

    private fun importSummary(name: String, report: ImportReport): String {
        val base = "« $name » importé : ${report.events} cours, ${report.occurrences} séances."
        return if (report.skipped.isEmpty()) {
            base
        } else {
            "$base ${report.skipped.size} élément(s) ignoré(s)."
        }
    }

    class Factory(private val core: Timewrap) : ViewModelProvider.Factory {
        @Suppress("UNCHECKED_CAST")
        override fun <T : ViewModel> create(modelClass: Class<T>): T = AppViewModel(core) as T
    }
}
