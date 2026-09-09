package app.timewrap

import android.content.Context
import androidx.lifecycle.ViewModel
import androidx.lifecycle.ViewModelProvider
import androidx.lifecycle.viewModelScope
import app.timewrap.core.CalendarKind
import app.timewrap.core.Category
import app.timewrap.core.Change
import app.timewrap.core.Conflict
import app.timewrap.core.ConflictPair
import app.timewrap.core.DayAgenda
import app.timewrap.core.EventDraft
import app.timewrap.core.NowView
import app.timewrap.core.Occurrence
import app.timewrap.core.PropertyKey
import app.timewrap.core.PropertyValue
import app.timewrap.core.Resolution
import app.timewrap.core.Rule
import app.timewrap.core.RuleSuggestion
import app.timewrap.core.Settings
import app.timewrap.core.SyncReport
import app.timewrap.core.Task
import app.timewrap.core.Timetable
import app.timewrap.core.Timewrap
import app.timewrap.notify.Notifications
import app.timewrap.notify.ReminderScheduler
import app.timewrap.sync.IcsFetcher
import app.timewrap.sync.SyncScheduler
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
 * que l'utilisateur tranche.
 */
data class PendingSave(
    val draft: EventDraft,
    val conflicts: List<Conflict>,
)

data class UiState(
    val timetable: Timetable? = null,
    val settings: Settings? = null,
    val now: NowView? = null,
    val today: Long = todayEpochDay(),
    val selectedDay: Long = todayEpochDay(),
    val weekStart: Long = todayEpochDay().startOfWeek(),
    /// Journées déjà chargées, indexées par `epochDay`.
    val days: Map<Long, DayAgenda> = emptyMap(),
    val categories: List<Category> = emptyList(),
    val rules: List<Rule> = emptyList(),
    val suggestions: List<RuleSuggestion> = emptyList(),
    /// Les champs structurés repérés dans l'emploi du temps.
    val propertyKeys: List<PropertyKey> = emptyList(),
    /// Les valeurs du champ ouvert dans l'écran des couleurs.
    val propertyValues: List<PropertyValue> = emptyList(),
    val conflicts: List<ConflictPair> = emptyList(),
    val muted: List<Occurrence> = emptyList(),
    /// Ce qu'a annoncé la dernière synchronisation lancée à la main.
    val lastChanges: List<Change> = emptyList(),
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

    val hasTimetable: Boolean get() = timetable != null
}

class AppViewModel(
    private val core: Timewrap,
    private val context: Context,
) : ViewModel() {

    private val _state = MutableStateFlow(UiState())
    val state: StateFlow<UiState> = _state.asStateFlow()

    /**
     * Ce que heurterait le brouillon en cours de saisie.
     *
     * Séparé de [state] parce qu'il change à chaque frappe : le garder à part
     * évite de recomposer tout l'écran pour un champ d'heure.
     */
    private val _draftConflicts = MutableStateFlow<List<Conflict>>(emptyList())
    val draftConflicts: StateFlow<List<Conflict>> = _draftConflicts.asStateFlow()

    /** Bornes déjà en cache, pour ne recharger que ce qui manque. */
    private var loadedFrom: Long = Long.MAX_VALUE
    private var loadedTo: Long = Long.MIN_VALUE

    init {
        refresh()
    }

    /** Recharge ce qui est affiché. */
    fun refresh() = launchCore {
        val snapshot = _state.value
        loadShell()
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

    // ------------------------------------------------------- emploi du temps

    fun importIcs(name: String, source: String, text: String) = launchCore {
        val report = core.importIcs(name, CalendarKind.ICS_FILE, source, text)
        val today = core.today()
        _state.update {
            it.copy(
                message = importSummary(name, report),
                lastChanges = report.changes,
                selectedDay = today,
                weekStart = today.startOfWeek(),
            )
        }
        afterTimetableChanged(today)
    }

    /** Enregistre une adresse d'abonnement et la télécharge dans la foulée. */
    fun subscribe(url: String) = launchCore {
        val settings = core.settings()
        core.updateSettings(settings.copy(sourceUrl = url.trim(), syncEnabled = true))
        syncNowInternal(announce = true)
    }

    fun syncNow() = launchCore { syncNowInternal(announce = true) }

    private suspend fun syncNowInternal(announce: Boolean) {
        val settings = core.settings()
        if (settings.sourceUrl.isBlank()) {
            _state.update { it.copy(message = "Aucune adresse d'abonnement enregistrée.") }
            return
        }

        val text = IcsFetcher.fetch(settings.sourceUrl).getOrElse { error ->
            _state.update {
                it.copy(message = "Synchronisation impossible : ${error.message ?: "erreur réseau"}")
            }
            return
        }

        val name = core.timetable()?.name ?: "Emploi du temps"
        val report = core.importIcs(name, CalendarKind.ICS_URL, settings.sourceUrl, text)

        if (announce && settings.notifyChanges && report.changes.isNotEmpty()) {
            Notifications.ensureChannels(context)
            Notifications.changes(context, report.changes)
        }
        _state.update {
            it.copy(message = syncSummary(report), lastChanges = report.changes)
        }
        afterTimetableChanged(_state.value.selectedDay)
    }

    fun renameTimetable(name: String) = launchCore {
        core.renameTimetable(name)
        loadShell()
    }

    fun setTimetableColor(color: UInt) = launchCore {
        core.setTimetableColor(color)
        reload(_state.value.selectedDay)
    }

    fun clearTimetable() = launchCore {
        core.clearTimetable()
        _state.update { it.copy(message = "Emploi du temps effacé.", lastChanges = emptyList()) }
        afterTimetableChanged(_state.value.selectedDay)
    }

    // -------------------------------------------------- événements et conflits

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
                message = saveSummary(
                    outcome.removed.toInt(),
                    outcome.hidden.toInt(),
                    outcome.shiftedMinutes,
                ),
                selectedDay = saved?.let { o -> epochDayOf(o.startUtc) } ?: it.selectedDay,
            )
        }
        reload(_state.value.selectedDay)
        ReminderScheduler.refresh(context)
    }

    /** Interroge le moteur pendant la saisie, sans rien écrire. */
    fun checkDraft(draft: EventDraft) {
        viewModelScope.launch {
            _draftConflicts.value = runCatching {
                withContext(Dispatchers.IO) { core.checkConflicts(draft) }
            }.getOrDefault(emptyList())
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
        ReminderScheduler.refresh(context)
    }

    fun setMuted(id: String, muted: Boolean) = launchCore {
        core.setOccurrenceMuted(id, muted)
        _state.update {
            it.copy(message = if (muted) "Séance masquée." else "Séance réaffichée.")
        }
        reload(_state.value.selectedDay)
        ReminderScheduler.refresh(context)
    }

    fun setOccurrenceCategory(id: String, categoryId: String?) = launchCore {
        core.setOccurrenceCategory(id, categoryId)
        reload(_state.value.selectedDay)
    }

    fun loadConflicts() = launchCore {
        val from = System.currentTimeMillis() / 1000
        val to = from + CONFLICT_HORIZON * 86_400
        _state.update {
            it.copy(
                conflicts = core.conflictsBetween(from, to),
                muted = core.mutedOccurrences(from, to),
            )
        }
    }

    // ------------------------------------------------------ couleurs et règles

    fun loadColors() = launchCore {
        _state.update {
            it.copy(
                propertyKeys = core.propertyKeys(),
                categories = core.categories(),
            )
        }
    }

    fun openProperty(key: String) = launchCore {
        _state.update { it.copy(propertyValues = core.propertyValues(key)) }
    }

    fun setPropertyColor(key: String, value: String, color: UInt) = launchCore {
        core.setPropertyColor(key, value, color)
        afterColorsChanged(key)
    }

    fun clearPropertyColor(key: String, value: String) = launchCore {
        core.clearPropertyColor(key, value)
        afterColorsChanged(key)
    }

    fun autoColorProperty(key: String) = launchCore {
        val posees = core.autoColorProperty(key)
        _state.update { it.copy(message = "$posees couleur(s) posée(s).") }
        afterColorsChanged(key)
    }

    private suspend fun afterColorsChanged(key: String) {
        _state.update {
            it.copy(
                propertyValues = core.propertyValues(key),
                propertyKeys = core.propertyKeys(),
                categories = core.categories(),
                rules = core.rules(),
            )
        }
        reload(_state.value.selectedDay)
    }

    fun loadRules() = launchCore {
        _state.update {
            it.copy(
                rules = core.rules(),
                categories = core.categories(),
                suggestions = core.ruleSuggestions(),
            )
        }
    }

    fun saveRule(rule: Rule) = launchCore {
        val saved = core.saveRule(rule)
        _state.update {
            it.copy(message = "Règle « ${saved.name} » : ${saved.matchCount} séance(s).")
        }
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
        _state.update {
            it.copy(message = "« $name » colorie ${rule.matchCount} séance(s).")
        }
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

    // ---------------------------------------------------------- choses à faire

    fun addTask(title: String, epochDay: Long) = launchCore {
        core.addTask(title, epochDay)
        reloadDay(epochDay)
    }

    fun setTaskDone(id: String, done: Boolean) = launchCore {
        core.setTaskDone(id, done)
        reloadDay(_state.value.selectedDay)
    }

    fun updateTask(id: String, title: String, notes: String) = launchCore {
        core.updateTask(id, title, notes)
        reloadDay(_state.value.selectedDay)
    }

    /** Repousse une tâche à demain — le geste le plus courant d'une liste. */
    fun postponeTask(id: String, toDay: Long) = launchCore {
        core.moveTask(id, toDay)
        reloadDay(_state.value.selectedDay)
    }

    fun deleteTask(id: String) = launchCore {
        core.deleteTask(id)
        reloadDay(_state.value.selectedDay)
    }

    // --------------------------------------------------------------- réglages

    fun updateSettings(settings: Settings) = launchCore {
        val saved = core.updateSettings(settings)
        _state.update { it.copy(settings = saved) }

        // Les réglages ne valent que par ce qu'ils déclenchent : on aligne
        // aussitôt la planification sur eux.
        Notifications.ensureChannels(context)
        SyncScheduler.apply(context, saved)
        ReminderScheduler.refresh(context)
    }

    fun dismissMessage() = _state.update { it.copy(message = null) }

    // ------------------------------------------------------------- chargement

    private suspend fun loadShell() {
        _state.update {
            it.copy(
                timetable = core.timetable(),
                settings = core.settings(),
                now = core.nowView(),
                today = core.today(),
                categories = core.categories(),
                propertyKeys = core.propertyKeys(),
            )
        }
    }

    private suspend fun loadRuleShell() {
        _state.update {
            it.copy(
                rules = core.rules(),
                categories = core.categories(),
                suggestions = core.ruleSuggestions(),
            )
        }
    }

    /** Après un import : tout est à revoir, rappels compris. */
    private suspend fun afterTimetableChanged(around: Long) {
        reload(around)
        ReminderScheduler.refresh(context)
        _state.value.settings?.let { SyncScheduler.apply(context, it) }
    }

    /** Après une modification du contenu, le cache de jours n'est plus fiable. */
    private suspend fun reload(around: Long) {
        loadShell()
        invalidateDays()
        loadAround(around)
        loadAround(_state.value.weekStart + 3)
    }

    /** Une tâche ne change qu'une journée : inutile de tout relire. */
    private suspend fun reloadDay(epochDay: Long) {
        val day = core.day(epochDay)
        val today = core.today()
        val alsoToday = if (epochDay == today) null else core.day(today)
        _state.update { current ->
            val days = current.days + (day.epochDay to day) +
                (alsoToday?.let { mapOf(it.epochDay to it) } ?: emptyMap())
            current.copy(days = days, now = core.nowView())
        }
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

        _state.update { current ->
            current.copy(days = current.days + loaded.associateBy { it.epochDay })
        }
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

    private fun epochDayOf(startUtc: Long): Long =
        java.time.Instant.ofEpochSecond(startUtc)
            .atZone(java.time.ZoneId.systemDefault())
            .toLocalDate()
            .toEpochDay()

    private fun importSummary(name: String, report: SyncReport): String {
        val base = "« $name » importé : ${report.import.events} cours, " +
            "${report.import.occurrences} séances."
        return if (report.import.skipped.isEmpty()) {
            base
        } else {
            "$base ${report.import.skipped.size} élément(s) ignoré(s)."
        }
    }

    private fun syncSummary(report: SyncReport): String = when (report.changes.size) {
        0 -> "À jour, rien n'a changé."
        1 -> report.changes.first().summary
        else -> "${report.changes.size} changements."
    }

    /** Ce qu'a coûté un enregistrement, quand il n'a pas été indolore. */
    private fun saveSummary(removed: Int, hidden: Int, shiftedMinutes: Long): String? = when {
        removed > 0 && hidden > 0 ->
            "Enregistré : $removed séance(s) supprimée(s), $hidden masquée(s)."
        removed > 0 -> "Enregistré : $removed séance(s) supprimée(s)."
        hidden > 0 -> "Enregistré : $hidden séance(s) masquée(s)."
        shiftedMinutes > 0 -> "Décalé de $shiftedMinutes min pour libérer le créneau."
        else -> null
    }

    class Factory(
        private val core: Timewrap,
        private val context: Context,
    ) : ViewModelProvider.Factory {
        @Suppress("UNCHECKED_CAST")
        override fun <T : ViewModel> create(modelClass: Class<T>): T =
            AppViewModel(core, context.applicationContext) as T
    }
}
