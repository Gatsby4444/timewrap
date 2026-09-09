package app.timewrap.notify

import android.app.AlarmManager
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import android.os.Build
import app.timewrap.TimewrapApp
import app.timewrap.core.Reminder
import app.timewrap.ui.deviceZone
import app.timewrap.ui.formatTime
import java.time.LocalDate
import java.time.LocalTime

/**
 * Pose les alarmes des rappels et du résumé du matin.
 *
 * Le cœur dit *quand* : « ce cours-là, à cet instant-là ». Ce module ne fait
 * que traduire cette liste en alarmes système, et la repose à chaque fois que
 * quelque chose bouge — import, réglage modifié, rappel déclenché, redémarrage.
 * Android n'accepte qu'un nombre limité d'alarmes exactes : on n'en garde
 * qu'une poignée d'avance, ce qui suffit puisqu'on les renouvelle sans cesse.
 */
object ReminderScheduler {

    /** Combien de cours à venir sont armés d'avance. */
    private const val MAX_REMINDERS = 8

    private const val ACTION_FIRE = "app.timewrap.REMINDER"
    private const val EXTRA_KIND = "kind"
    private const val EXTRA_TITLE = "title"
    private const val EXTRA_BODY = "body"
    private const val EXTRA_INDEX = "index"

    private const val KIND_COURSE = "course"
    private const val KIND_DIGEST = "digest"

    /** Code de requête du résumé, hors de la plage des rappels. */
    private const val REQUEST_DIGEST = 900

    /**
     * Reprend tout : annule les alarmes en place, puis repose celles qui ont
     * encore lieu d'être.
     *
     * Idempotent par construction — c'est ce qui permet de l'appeler à tout
     * bout de champ sans se demander si c'est utile.
     */
    fun refresh(context: Context) {
        val app = context.applicationContext as TimewrapApp
        val alarms = context.getSystemService(AlarmManager::class.java) ?: return

        for (index in 0 until MAX_REMINDERS) {
            alarms.cancel(pending(context, index, null))
        }
        alarms.cancel(pending(context, REQUEST_DIGEST, null))

        val settings = runCatching { app.core.settings() }.getOrNull() ?: return

        if (settings.remindersEnabled) {
            val reminders = runCatching { app.core.reminders(MAX_REMINDERS.toUInt()) }
                .getOrDefault(emptyList())
            reminders.forEachIndexed { index, reminder ->
                schedule(context, alarms, index, reminder, settings.reminderLeadMinutes.toInt())
            }
        }

        if (settings.digestEnabled) {
            scheduleDigest(context, alarms, settings.digestMinutes.toInt())
        }
    }

    private fun schedule(
        context: Context,
        alarms: AlarmManager,
        index: Int,
        reminder: Reminder,
        leadMinutes: Int,
    ) {
        val body = buildString {
            append(reminder.startUtc.formatTime())
            if (reminder.location.isNotBlank()) {
                append(" · ")
                append(reminder.location)
            }
        }
        val title = when {
            leadMinutes <= 0 -> reminder.title
            else -> "Dans $leadMinutes min : ${reminder.title}"
        }

        val intent = Intent(context, ReminderReceiver::class.java)
            .setAction(ACTION_FIRE)
            .putExtra(EXTRA_KIND, KIND_COURSE)
            .putExtra(EXTRA_INDEX, index)
            .putExtra(EXTRA_TITLE, title)
            .putExtra(EXTRA_BODY, body)

        set(alarms, reminder.triggerUtc * 1000, pending(context, index, intent))
    }

    private fun scheduleDigest(context: Context, alarms: AlarmManager, minutesAfterMidnight: Int) {
        val time = LocalTime.of(minutesAfterMidnight / 60, minutesAfterMidnight % 60)
        val today = LocalDate.now(deviceZone).atTime(time)
        // Si l'heure est déjà passée, le prochain résumé est celui de demain.
        val next = if (today.isAfter(java.time.LocalDateTime.now(deviceZone))) {
            today
        } else {
            today.plusDays(1)
        }

        val intent = Intent(context, ReminderReceiver::class.java)
            .setAction(ACTION_FIRE)
            .putExtra(EXTRA_KIND, KIND_DIGEST)

        set(
            alarms,
            next.atZone(deviceZone).toInstant().toEpochMilli(),
            pending(context, REQUEST_DIGEST, intent),
        )
    }

    /**
     * Une alarme exacte quand le système l'autorise, approchée sinon.
     *
     * Un rappel « quinze minutes avant » perd tout son sens s'il arrive avec un
     * quart d'heure de retard, mais mieux vaut un rappel approximatif que pas
     * de rappel du tout.
     */
    private fun set(alarms: AlarmManager, triggerAtMillis: Long, pendingIntent: PendingIntent) {
        val exact = Build.VERSION.SDK_INT < Build.VERSION_CODES.S || alarms.canScheduleExactAlarms()
        runCatching {
            if (exact) {
                alarms.setExactAndAllowWhileIdle(
                    AlarmManager.RTC_WAKEUP,
                    triggerAtMillis,
                    pendingIntent,
                )
            } else {
                alarms.set(AlarmManager.RTC_WAKEUP, triggerAtMillis, pendingIntent)
            }
        }
    }

    /**
     * `null` pour l'intention sert à retrouver une alarme existante afin de
     * l'annuler : seuls le code de requête et l'action comptent pour l'égalité.
     */
    private fun pending(context: Context, requestCode: Int, intent: Intent?): PendingIntent {
        val target = intent ?: Intent(context, ReminderReceiver::class.java).setAction(ACTION_FIRE)
        return PendingIntent.getBroadcast(
            context,
            requestCode,
            target,
            PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE,
        )
    }

    /** Ce que le récepteur a besoin de savoir de l'intention reçue. */
    internal fun isDigest(intent: Intent): Boolean =
        intent.getStringExtra(EXTRA_KIND) == KIND_DIGEST

    internal fun title(intent: Intent): String = intent.getStringExtra(EXTRA_TITLE).orEmpty()

    internal fun body(intent: Intent): String = intent.getStringExtra(EXTRA_BODY).orEmpty()

    internal fun index(intent: Intent): Int = intent.getIntExtra(EXTRA_INDEX, 0)
}
