package app.timewrap.notify

import android.Manifest
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.os.Build
import androidx.core.app.NotificationCompat
import androidx.core.app.NotificationManagerCompat
import androidx.core.content.ContextCompat
import app.timewrap.MainActivity
import app.timewrap.R
import app.timewrap.core.Change

/**
 * Les notifications de l'application, et rien d'autre.
 *
 * Trois canaux distincts parce que trois urgences distinctes : un cours qui
 * change de salle doit pouvoir réveiller, un résumé du matin non. Les séparer,
 * c'est laisser l'utilisateur couper l'un sans perdre l'autre — ce que le
 * réglage système fait très bien à notre place.
 */
object Notifications {

    const val CHANNEL_CHANGES = "changements"
    const val CHANNEL_REMINDERS = "rappels"
    const val CHANNEL_DIGEST = "resume"

    /** Ce qu'une notification demande d'ouvrir, passé à l'activité. */
    const val EXTRA_OPEN = "app.timewrap.open"
    const val OPEN_HISTORY = "history"

    private const val ID_CHANGES = 1
    private const val ID_DIGEST = 2
    private const val GROUP_CHANGES = "app.timewrap.changes"
    /** Au-delà, une notification par changement deviendrait du bruit. */
    private const val MAX_CHANGE_NOTIFICATIONS = 8
    /** Les changements d'une synchronisation : de cette base à base + max. */
    private const val ID_CHANGE_BASE = 100
    /** Les rappels s'empilent : chacun le sien, à partir de cette base. */
    private const val ID_REMINDER_BASE = 1000

    fun ensureChannels(context: Context) {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.O) return
        val manager = context.getSystemService(NotificationManager::class.java) ?: return

        manager.createNotificationChannel(
            NotificationChannel(
                CHANNEL_CHANGES,
                "Changements d'emploi du temps",
                NotificationManager.IMPORTANCE_HIGH,
            ).apply {
                description = "Cours déplacé, salle changée, séance annulée."
            },
        )
        manager.createNotificationChannel(
            NotificationChannel(
                CHANNEL_REMINDERS,
                "Rappels avant les cours",
                NotificationManager.IMPORTANCE_HIGH,
            ).apply {
                description = "Quelques minutes avant chaque séance."
            },
        )
        manager.createNotificationChannel(
            NotificationChannel(
                CHANNEL_DIGEST,
                "Résumé du matin",
                NotificationManager.IMPORTANCE_DEFAULT,
            ).apply {
                description = "Ce qui vous attend aujourd'hui, et ce qu'il reste à faire."
            },
        )
    }

    /** Vrai si l'utilisateur nous laisse notifier — sur Android 13 et plus. */
    fun allowed(context: Context): Boolean =
        Build.VERSION.SDK_INT < Build.VERSION_CODES.TIRAMISU ||
            ContextCompat.checkSelfPermission(
                context,
                Manifest.permission.POST_NOTIFICATIONS,
            ) == PackageManager.PERMISSION_GRANTED

    /**
     * Annonce ce qui a bougé après une synchronisation.
     *
     * Une notification par changement, regroupées sous un résumé : chacune
     * garde sa phrase entière, dépliable. Avant, tout tenait dans une seule
     * notification à lignes, et le système coupait chaque ligne au bord de
     * l'écran — on savait qu'un cours avait bougé, pas où ni quand.
     *
     * Toutes ouvrent l'historique, qui garde l'avant et l'après de chaque
     * changement après que la notification a été balayée.
     *
     * Le texte vient du cœur, déjà rédigé : l'interface n'a pas à savoir
     * conjuguer « est annulé » ni à formater une date.
     */
    fun changes(context: Context, changes: List<Change>) {
        if (changes.isEmpty()) return
        val manager = NotificationManagerCompat.from(context)
        // Les notifications de la synchronisation précédente racontent un état
        // dépassé : on les remplace plutôt que d'empiler.
        (0..MAX_CHANGE_NOTIFICATIONS).forEach { manager.cancel(ID_CHANGE_BASE + it) }

        val title = when (changes.size) {
            1 -> "Un changement dans votre emploi du temps"
            else -> "${changes.size} changements dans votre emploi du temps"
        }
        val shown = changes.take(MAX_CHANGE_NOTIFICATIONS)
        val rest = changes.size - shown.size

        shown.forEachIndexed { index, change ->
            post(
                context,
                ID_CHANGE_BASE + index,
                builder(context, CHANNEL_CHANGES, openHistory = true)
                    .setContentTitle(change.title)
                    .setContentText(change.summary)
                    .setStyle(NotificationCompat.BigTextStyle().bigText(change.summary))
                    .setGroup(GROUP_CHANGES)
                    .setWhen(System.currentTimeMillis())
                    // Une seule sonnerie pour toute la synchronisation : c'est le
                    // résumé qui alerte.
                    .setGroupAlertBehavior(NotificationCompat.GROUP_ALERT_SUMMARY)
                    .setSortKey("%03d".format(index))
                    .build(),
            )
        }

        val inbox = NotificationCompat.InboxStyle().setBigContentTitle(title)
        shown.forEach { inbox.addLine(it.summary) }
        inbox.setSummaryText(
            if (rest > 0) "et $rest de plus — tout est dans l'historique" else "Ouvrir l'historique",
        )
        post(
            context,
            ID_CHANGES,
            builder(context, CHANNEL_CHANGES, openHistory = true)
                .setContentTitle(title)
                .setContentText(changes.first().summary)
                .setStyle(inbox)
                .setGroup(GROUP_CHANGES)
                .setGroupSummary(true)
                .setNumber(changes.size)
                .build(),
        )
    }

    /** « Dans 15 min : Analyse — C204 ». */
    fun reminder(context: Context, index: Int, title: String, body: String) {
        post(
            context,
            ID_REMINDER_BASE + index,
            builder(context, CHANNEL_REMINDERS)
                .setContentTitle(title)
                .setContentText(body)
                .build(),
        )
    }

    fun digest(context: Context, title: String, body: String) {
        post(
            context,
            ID_DIGEST,
            builder(context, CHANNEL_DIGEST)
                .setContentTitle(title)
                .setContentText(body)
                .setStyle(NotificationCompat.BigTextStyle().bigText(body))
                .build(),
        )
    }

    private fun builder(context: Context, channel: String, openHistory: Boolean = false) =
        NotificationCompat.Builder(context, channel)
            .setSmallIcon(R.drawable.ic_notification)
            .setContentIntent(openApp(context, openHistory))
            .setAutoCancel(true)
            .setCategory(NotificationCompat.CATEGORY_EVENT)

    private fun openApp(context: Context, openHistory: Boolean): PendingIntent {
        val intent = Intent(context, MainActivity::class.java)
            .setFlags(Intent.FLAG_ACTIVITY_CLEAR_TOP or Intent.FLAG_ACTIVITY_SINGLE_TOP)
        if (openHistory) intent.putExtra(EXTRA_OPEN, OPEN_HISTORY)
        return PendingIntent.getActivity(
            context,
            // Deux intentions qui ne diffèrent que par un extra sont la même pour
            // le système : un code distinct garde l'extra.
            if (openHistory) 1 else 0,
            intent,
            PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE,
        )
    }

    private fun post(context: Context, id: Int, notification: android.app.Notification) {
        if (!allowed(context)) return
        // La permission vient d'être vérifiée ; le catch couvre le cas où
        // l'utilisateur la retire entre-temps, que le système signale par une
        // exception plutôt que par un retour.
        runCatching {
            NotificationManagerCompat.from(context).notify(id, notification)
        }
    }
}
