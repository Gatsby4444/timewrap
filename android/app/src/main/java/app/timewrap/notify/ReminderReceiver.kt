package app.timewrap.notify

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import app.timewrap.TimewrapApp
import app.timewrap.ui.formatTime

/**
 * Déclenche un rappel, ou le résumé du matin.
 *
 * Une fois la notification postée, les alarmes sont reposées : celle qui vient
 * de partir laisse la place à la suivante, et la file d'avance reste pleine.
 */
class ReminderReceiver : BroadcastReceiver() {

    override fun onReceive(context: Context, intent: Intent) {
        Notifications.ensureChannels(context)

        if (ReminderScheduler.isDigest(intent)) {
            postDigest(context)
        } else {
            Notifications.reminder(
                context,
                ReminderScheduler.index(intent),
                ReminderScheduler.title(intent),
                ReminderScheduler.body(intent),
            )
        }

        ReminderScheduler.refresh(context)
    }

    /**
     * « Aujourd'hui : 4 cours, à partir de 08:00. 2 choses à faire. »
     *
     * Les chiffres viennent du cœur ; seule la phrase est écrite ici, parce
     * qu'elle ne sert qu'à cet écran-là.
     */
    private fun postDigest(context: Context) {
        val core = (context.applicationContext as TimewrapApp).core
        val view = runCatching { core.nowView() }.getOrNull() ?: return
        val today = runCatching { core.today() }.getOrNull() ?: return
        val day = runCatching { core.day(today) }.getOrNull() ?: return

        val courses = day.occurrences.filter { !it.allDay && !it.cancelled }
        val first = courses.minByOrNull { it.startUtc }

        val cours = when (courses.size) {
            0 -> "Aucun cours aujourd'hui."
            1 -> "1 cours aujourd'hui, à ${first?.startUtc?.formatTime()}."
            else -> "${courses.size} cours aujourd'hui, à partir de ${first?.startUtc?.formatTime()}."
        }
        val taches = when {
            view.pendingTasks == 0u -> "Rien à faire de noté."
            view.lateTasks > 0u ->
                "${view.pendingTasks} chose(s) à faire, dont ${view.lateTasks} en retard."
            else -> "${view.pendingTasks} chose(s) à faire."
        }

        Notifications.digest(context, "Votre journée", "$cours\n$taches")
    }
}
