package app.timewrap

import android.app.Application
import app.timewrap.core.Timewrap
import java.time.ZoneId

/**
 * Ouvre le cœur une fois pour toute la durée de vie du processus.
 *
 * La base vit dans le stockage privé de l'application : rien ne sort de
 * l'appareil, et aucune permission n'est nécessaire.
 */
class TimewrapApp : Application() {

    val core: Timewrap by lazy {
        val dbPath = getDatabasePath("timewrap.db").also { it.parentFile?.mkdirs() }.absolutePath
        Timewrap.open(dbPath, ZoneId.systemDefault().id).also {
            // Si l'application n'a pas été ouverte depuis longtemps, l'horizon
            // des récurrences développées peut être devenu trop proche.
            runCatching { it.ensureHorizon() }
        }
    }
}
