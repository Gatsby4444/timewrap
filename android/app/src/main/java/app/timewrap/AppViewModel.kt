package app.timewrap

import androidx.lifecycle.ViewModel
import androidx.lifecycle.ViewModelProvider
import androidx.lifecycle.viewModelScope
import app.timewrap.core.Calendar
import app.timewrap.core.CalendarKind
import app.timewrap.core.CalendarSummary
import app.timewrap.core.Category
import app.timewrap.core.Conflict
import app.timewrap.core.ConflictPair
import app.timewrap.core.DayAgenda
import app.timewrap.core.EventDraft
import app.timewrap.core.ImportReport
import app.timewrap.core.NowView
import app.timewrap.core.Occurrence
import app.timewrap.core.Resolution
import app.timewrap.core.Rule
import app.timewrap.core.RuleSuggestion
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

/** Profondeur du gestionnaire de conflits, en jours. */
private const val CONFLICT_HORIZON = 60L

/**
 * Un enregistrement suspendu par un chevauchement.
 *
 * Le cœur a refusé d'écrire et rendu la liste des heurts ; il faut maintenant
 * que l'utilisateur tranche. Tant que cet objet existe, la boîte de dialogue de
 * résolution est à l'écran.
 */
data class PendingSave(
    val draft: EventDraft,
    val conflicts: List<Conflict>,
)

data class UiState(
    val calendars: List<Calendar> = emptyList(),
    val summaries: List<CalendarSummary> = emptyList(),
    val categories: List<Category> = emptyList(),
    val rules: List<Rule> = emptyList(),
    val suggestions: List<RuleSuggestion> = emptyList(),
    val conflicts: List<ConflictPair> = emptyList(),
    /// Séances écartées à la main, que le gestionnaire propose de rappeler.
    val muted: List<Occurrence> = emptyList(),
    /// Agenda ouvert seul, ou `null` pour la vue d'ensemble.
    val scope: String? = null,
    val now: NowView? = null,
    val selectedDay: Long = todayEpochDay(),
    val weekStart: Long = todayEpochDay().startOfWeek(),
    /// Journées déjà chargées, indexées par `epochDay`.
    val days: Map<Long, DayAgenda> = emptyMap(),
    val busy: Boolean = false,
    /// Message éphémère : bilan d'import ou erreur.
    val message: String? = null,
    val pendingSave: PendingSave? = null,
) {
    fun day(epochDay: Long): DayAgenda? = days[epochDay]

    /** Les sept jours d'une semaine, ou `null` tant qu'il en manque un. */
    fun week(start: Long): List<DayAgenda>? {
        val found = (0L until 7L).mapNotNull { days[start + it] }
        return found.takeIf { it.size == 7 }
    }

    /** L'agenda actuellement ouvert, s'il y en a un. */
    val openCalendar: Calendar? get() = calendars.firstOrNull { it.id == scope }

    /** Où écrire un nouvel événement par défaut : l'agenda ouvert, ou le premier local. */
    val defaultTarget: Calendar?
        get() = openCalendar
            ?: calendars.firstOrNull { it.kind == CalendarKind.LOCAL }
            ?: calendars.firstOrNull()

    val totalConflicts: Int get() = summaries.sumOf { it.conflicts.toInt() }
}

class AppViewModel(private val core: Timewrap) : ViewModel() {

    private val _state = MutableStateFlow(UiState())
    val state: StateFlow<UiState> = _state.asStateFlow()

    /**
     * Ce que heurterait le brouillon en cours de saisie.
     *
     * Séparé de [state] parce qu'il change à chaque frappe : le garder à part
     * évite de recomposer tout l'écran d'accueil pour un champ d'heure.
     */
    private val _draftConflicts = MutableStateFlow<List<Conflict>>(emptyList())
    val draftConflicts: StateFlow<List<Conflict>> = _draftConflicts.asStateFlow()

    /** Bornes déjà en cache, pour ne recharger que ce qui manque. */
    private var loadedFrom: Long = Long.MAX_VALUE
    private var loadedTo: Long = Long.MIN_VALUE

    init {
        refresh()
    }

    /** La portée telle que le cœur l'attend : une liste, ou rien. */
    private fun scopeIds(): List<String>? = _state.value.scope?.let { listOf(it) }

    /** Recharge ce qui est affiché : agendas, vue « maintenant », jours en cache. */
    fun refresh() = launchCore {
        val snapshot = _state.value
        loadShell()
        invalidateDays()
        loadAround(snapshot.selectedDay)
        loadAround(snapshot.weekStart + 3)
    }

    // ------------------------------------------------------------- navigation

    /**
     * Ouvre un agenda seul, ou revient à la vue d'ensemble avec `null`.
     *
     * Le cache de journées est indexé par jour, pas par portée : changer de
     * dossier doit donc le vider, sans quoi la vue Jour montrerait encore le
     * contenu du dossier précédent.
     */
    fun openCalendar(calendarId: String?) = launchCore {
        _state.update { it.copy(scope = calendarId) }
        reload(_state.value.selectedDay)
    }

    fun selectDay(epochDay: Long) {
        _state.update { it.copy(selectedDay = epochDay) }
        launchCore { loadAround(epochDay) }
    }

    fun selectWeek(weekStart: Long) {
        _state.update { it.copy(weekStart = weekStart) }
        launchCore { loadAround(weekStart + 3) }
    }

    // ---------------------------------------------------------------- agendas

    fun importIcs(name: String, source: String, text: String) = launchCore {
        val report = core.importIcs(name, CalendarKind.ICS_FILE, source, text)
        val today = todayEpochDay()
        _state.update {
            it.copy(
                message = importSummary(name, report),
                scope = null,
                selectedDay = today,
                weekStart = today.startOfWeek(),
            )
        }
        reload(today)
    }

    fun createCalendar(name: String) = launchCore {
        val calendar = core.createCalendar(name, null)
        _state.update { it.copy(message = "Agenda « ${calendar.name} » créé.") }
        reload(_state.value.selectedDay)
    }

    fun setVisible(calendarId: String, visible: Boolean) = launchCore {
        core.setCalendarVisible(calendarId, visible)
        reload(_state.value.selectedDay)
    }

    fun setCalendarColor(calendarId: String, color: UInt) = launchCore {
        core.setCalendarColor(calendarId, color)
        reload(_state.value.selectedDay)
    }

    fun rename(calendarId: String, name: String) = launchCore {
        core.renameCalendar(calendarId, name)
        reload(_state.value.selectedDay)
    }

    fun moveCalendar(calendarId: String, target: Int) = launchCore {
        core.moveCalendar(calendarId, target)
        reload(_state.value.selectedDay)
    }

    fun delete(calendarId: String) = launchCore {
        core.deleteCalendar(calendarId)
        // L'agenda ouvert vient peut-être de disparaître sous nos pieds.
        _state.update { if (it.scope == calendarId) it.copy(scope = null) else it }
        reload(_state.value.selectedDay)
    }

    // -------------------------------------------------- événements et conflits

    /**
     * Enregistre un événement, en laissant le cœur arbitrer.
     *
     * Premier appel sans résolution : s'il y a chevauchement, rien n'est écrit
     * et la question remonte à l'écran. L'utilisateur choisit, et
     * [resolvePending] rappelle le cœur avec sa décision.
     */
    fun saveEvent(draft: EventDraft, resolution: Resolution = Resolution.CANCEL) = launchCore {
        val outcome = core.saveEvent(draft, resolution)
        if (outcome.blocked) {
            _state.update { it.copy(pendingSave = PendingSave(draft, outcome.conflicts)) }
            return@launchCore
        }
        val saved = outcome.saved
        _state.update {
            it.copy(
                pendingSave = null,
                message = saveSummary(outcome.removed.toInt(), outcome.hidden.toInt(), outcome.shiftedMinutes),
                selectedDay = saved?.let { occurrence -> epochDayOf(occurrence) } ?: it.selectedDay,
            )
        }
        reload(_state.value.selectedDay)
    }

    /** Interroge le moteur pendant la saisie, sans rien écrire. */
    fun checkDraft(draft: EventDraft) {
        viewModelScope.launch {
            val conflicts = runCatching {
                withContext(Dispatchers.IO) { core.checkConflicts(draft) }
            }.getOrDefault(emptyList())
            _draftConflicts.value = conflicts
        }
    }

    fun resolvePending(resolution: Resolution) {
        val pending = _state.value.pendingSave ?: return
        saveEvent(pending.draft, resolution)
    }

    fun cancelPending() = _state.update { it.copy(pendingSave = null) }

    fun deleteEvent(id: String) = launchCore {
        core.deleteEvent(id)
        reload(_state.value.selectedDay)
    }

    fun setMuted(id: String, muted: Boolean) = launchCore {
        core.setOccurrenceMuted(id, muted)
        _state.update {
            it.copy(message = if (muted) "Séance masquée." else "Séance réaffichée.")
        }
        reload(_state.value.selectedDay)
    }

    fun setOccurrenceCategory(id: String, categoryId: String?) = launchCore {
        core.setOccurrenceCategory(id, categoryId)
        reload(_state.value.selectedDay)
    }

    /** Charge le gestionnaire de conflits sur les deux mois qui viennent. */
    fun loadConflicts() = launchCore {
        val from = System.currentTimeMillis() / 1000
        val to = from + CONFLICT_HORIZON * 86_400
        val conflicts = core.conflictsBetween(from, to, scopeIds())
        val muted = core.mutedOccurrences(from, to)
        _state.update { it.copy(conflicts = conflicts, muted = muted) }
    }

    // ------------------------------------------------- catégories et règles

    fun loadRules() = launchCore { loadRuleShell() }

    fun saveRule(rule: Rule) = launchCore {
        val saved = core.saveRule(rule)
        _state.update { it.copy(message = "Règle « ${saved.name} » : ${saved.matchCount} séance(s).") }
        loadRuleShell()
        reload(_state.value.selectedDay)
    }

    fun deleteRule(id: String) = launchCore {
        core.deleteRule(id)
        loadRuleShell()
        reload(_state.value.selectedDay)
    }

    fun acceptSuggestion(suggestion: RuleSuggestion, name: String, label: String) = launchCore {
        val rule = core.acceptSuggestion(suggestion, name, label, null)
        _state.update { it.copy(message = "« $name » applique sa couleur à ${rule.matchCount} séance(s).") }
        loadRuleShell()
        reload(_state.value.selectedDay)
    }

    fun createCategory(name: String, label: String) = launchCore {
        core.createCategory(name, label, null)
        loadRuleShell()
    }

    fun updateCategory(id: String, name: String, label: String, color: UInt) = launchCore {
        core.updateCategory(id, name, label, color)
        loadRuleShell()
        reload(_state.value.selectedDay)
    }

    fun deleteCategory(id: String) = launchCore {
        core.deleteCategory(id)
        loadRuleShell()
        reload(_state.value.selectedDay)
    }

    fun dismissMessage() = _state.update { it.copy(message = null) }

    // ------------------------------------------------------------- chargement

    /**
     * Ce qui ne dépend pas de la journée consultée.
     *
     * Les règles en font partie : l'écran d'accueil annonce combien il y en a,
     * et un compte qui n'apparaîtrait qu'après avoir ouvert leur écran mentirait
     * au premier coup d'œil.
     */
    private suspend fun loadShell() {
        val calendars = core.calendars()
        val summaries = core.calendarSummaries()
        val now = core.nowView(scopeIds())
        val categories = core.categories()
        val rules = core.rules()
        _state.update {
            it.copy(
                calendars = calendars,
                summaries = summaries,
                now = now,
                categories = categories,
                rules = rules,
            )
        }
    }

    private suspend fun loadRuleShell() {
        val rules = core.rules()
        val categories = core.categories()
        val suggestions = core.ruleSuggestions(scopeIds())
        _state.update {
            it.copy(rules = rules, categories = categories, suggestions = suggestions)
        }
    }

    /** Après une modification du contenu, le cache de jours n'est plus fiable. */
    private suspend fun reload(around: Long) {
        loadShell()
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
        val loaded = core.days(missing.first, count.toUInt(), scopeIds())

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

    private fun epochDayOf(occurrence: Occurrence): Long =
        java.time.Instant.ofEpochSecond(occurrence.startUtc)
            .atZone(java.time.ZoneId.systemDefault())
            .toLocalDate()
            .toEpochDay()

    private fun importSummary(name: String, report: ImportReport): String {
        val base = "« $name » importé : ${report.events} cours, ${report.occurrences} séances."
        return if (report.skipped.isEmpty()) {
            base
        } else {
            "$base ${report.skipped.size} élément(s) ignoré(s)."
        }
    }

    /** Ce qu'a coûté un enregistrement, quand il n'a pas été indolore. */
    private fun saveSummary(removed: Int, hidden: Int, shiftedMinutes: Long): String? = when {
        removed > 0 && hidden > 0 -> "Enregistré : $removed séance(s) supprimée(s), $hidden masquée(s)."
        removed > 0 -> "Enregistré : $removed séance(s) supprimée(s)."
        hidden > 0 -> "Enregistré : $hidden séance(s) masquée(s)."
        shiftedMinutes > 0 -> "Décalé de ${shiftedMinutes} min pour libérer le créneau."
        else -> null
    }

    class Factory(private val core: Timewrap) : ViewModelProvider.Factory {
        @Suppress("UNCHECKED_CAST")
        override fun <T : ViewModel> create(modelClass: Class<T>): T = AppViewModel(core) as T
    }
}
