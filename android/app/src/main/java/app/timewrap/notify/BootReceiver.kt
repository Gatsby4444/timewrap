package app.timewrap.notify

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import app.timewrap.TimewrapApp
import app.timewrap.sync.SyncScheduler

/**
 * Repose ce qu'un redémarrage efface.
 *
 * Android oublie les alarmes au reboot, et une mise à jour de l'application les
 * emporte aussi. Sans ce récepteur, les rappels s'arrêteraient silencieusement
 * — la pire façon de tomber en panne, puisqu'on ne s'en aperçoit qu'après avoir
 * manqué un cours.
 */
class BootReceiver : BroadcastReceiver() {

    override fun onReceive(context: Context, intent: Intent) {
        Notifications.ensureChannels(context)
        ReminderScheduler.refresh(context)

        val settings = runCatching {
            (context.applicationContext as TimewrapApp).core.settings()
        }.getOrNull() ?: return
        SyncScheduler.apply(context, settings)
    }
}
