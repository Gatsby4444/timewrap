package app.timewrap.sync

import android.content.Context
import androidx.work.Constraints
import androidx.work.CoroutineWorker
import androidx.work.ExistingPeriodicWorkPolicy
import androidx.work.NetworkType
import androidx.work.PeriodicWorkRequestBuilder
import androidx.work.WorkManager
import androidx.work.WorkerParameters
import app.timewrap.TimewrapApp
import app.timewrap.core.CalendarKind
import app.timewrap.core.Settings
import app.timewrap.core.SyncReport
import app.timewrap.notify.Notifications
import app.timewrap.notify.ReminderScheduler
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import java.util.concurrent.TimeUnit

/**
 * Retélécharge l'abonnement et annonce ce qui a changé.
 *
 * Le travail utile tient en trois lignes — récupérer, importer, notifier — parce
 * que la comparaison entre l'ancien et le nouvel emploi du temps est faite par
 * le cœur, qui rend des phrases déjà écrites.
 */
class SyncWorker(context: Context, params: WorkerParameters) :
    CoroutineWorker(context, params) {

    override suspend fun doWork(): Result = withContext(Dispatchers.IO) {
        val app = applicationContext as TimewrapApp
        val settings = runCatching { app.core.settings() }.getOrNull()
            ?: return@withContext Result.success()

        if (settings.sourceUrl.isBlank()) return@withContext Result.success()

        val text = IcsFetcher.fetch(settings.sourceUrl).getOrElse {
            // Un ENT injoignable n'est pas une erreur de l'application : on
            // retentera au prochain créneau plutôt que d'échouer bruyamment.
            return@withContext Result.retry()
        }

        val report = runCatching {
            app.core.importIcs(
                app.core.timetable()?.name ?: "Emploi du temps",
                CalendarKind.ICS_URL,
                settings.sourceUrl,
                text,
            )
        }.getOrElse { return@withContext Result.failure() }

        announce(report, settings)
        ReminderScheduler.refresh(applicationContext)
        Result.success()
    }

    private fun announce(report: SyncReport, settings: Settings) {
        if (!settings.notifyChanges || report.changes.isEmpty()) return
        Notifications.ensureChannels(applicationContext)
        Notifications.changes(applicationContext, report.changes)
    }
}

/**
 * Décide quand le travail ci-dessus tourne.
 *
 * WorkManager ne descend pas sous le quart d'heure et regroupe les réveils :
 * c'est exactement ce qu'on veut pour un emploi du temps, qui ne change pas à la
 * minute près et dont la synchronisation ne doit pas coûter de batterie.
 */
object SyncScheduler {

    private const val WORK_NAME = "timewrap-sync"

    /** Aligne la planification sur les réglages : programme, ou arrête. */
    fun apply(context: Context, settings: Settings) {
        val manager = WorkManager.getInstance(context)
        if (!settings.syncEnabled || settings.sourceUrl.isBlank()) {
            manager.cancelUniqueWork(WORK_NAME)
            return
        }

        val request = PeriodicWorkRequestBuilder<SyncWorker>(
            settings.syncIntervalHours.toLong(),
            TimeUnit.HOURS,
        )
            .setConstraints(
                Constraints.Builder()
                    .setRequiredNetworkType(NetworkType.CONNECTED)
                    .build(),
            )
            .build()

        manager.enqueueUniquePeriodicWork(
            WORK_NAME,
            // L'intervalle fait partie de la requête : le changer doit remplacer
            // la planification en place, pas s'y ajouter.
            ExistingPeriodicWorkPolicy.UPDATE,
            request,
        )
    }
}
