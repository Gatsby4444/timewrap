package app.timewrap.sync

import java.net.HttpURLConnection
import java.net.SocketTimeoutException
import java.net.URL
import java.net.UnknownHostException
import java.security.GeneralSecurityException
import java.util.zip.GZIPInputStream
import javax.net.ssl.SSLException

/**
 * Un téléchargement qui a échoué, dit en français.
 *
 * Le message d'origine est conservé en cause : il n'a rien à faire sous les yeux
 * de quelqu'un qui veut son emploi du temps, mais tout à faire dans un journal.
 */
class IcsFetchError(message: String, cause: Throwable) : Exception(message, cause)

/**
 * Téléchargement d'un abonnement `.ics`.
 *
 * Volontairement sans bibliothèque : un GET, quelques redirections, un délai
 * d'attente. Ajouter un client HTTP complet pour ça alourdirait l'APK sans rien
 * apporter, et c'est du code que l'on veut pouvoir lire d'un trait le jour où un
 * ENT répond de travers.
 */
object IcsFetcher {

    private const val TIMEOUT_MS = 20_000
    /** Un emploi du temps d'année tient largement dedans ; au-delà, c'est autre chose. */
    private const val MAX_BYTES = 8 * 1024 * 1024
    private const val MAX_REDIRECTS = 5

    /**
     * Récupère le contenu d'une URL d'abonnement.
     *
     * Les adresses `webcal://` sont l'usage courant des ENT : c'est du HTTPS
     * déguisé, on le traduit plutôt que de le refuser.
     */
    fun fetch(rawUrl: String): Result<String> =
        runCatching { download(rawUrl) }
            .recoverCatching { throw IcsFetchError(explain(it), it) }

    /**
     * Traduit un échec réseau en une phrase qui dit quoi faire.
     *
     * Un « CertPathValidatorException » n'apprend rien à personne, et surtout
     * pas que le problème vient du téléphone et non de l'ENT.
     */
    private fun explain(error: Throwable): String = when (error) {
        // Nos propres refus sont déjà écrits pour être lus.
        is IllegalStateException -> error.message ?: "réponse inattendue de l'ENT"

        is SSLException, is GeneralSecurityException ->
            "connexion sécurisée refusée. Android ne reconnaît pas l'autorité qui " +
                "a signé le certificat de l'ENT — vérifiez d'abord que la date et " +
                "l'heure de l'appareil sont justes."

        is UnknownHostException ->
            "site introuvable. Vérifiez le réseau, et l'orthographe de l'adresse."

        is SocketTimeoutException -> "l'ENT n'a pas répondu à temps."

        else -> error.message ?: error.toString()
    }

    private fun download(rawUrl: String): String {
        var url = URL(normalize(rawUrl))

        repeat(MAX_REDIRECTS + 1) {
            val connection = (url.openConnection() as HttpURLConnection).apply {
                requestMethod = "GET"
                connectTimeout = TIMEOUT_MS
                readTimeout = TIMEOUT_MS
                // Certains ENT servent du HTML si on ne demande rien de précis.
                setRequestProperty("Accept", "text/calendar, text/plain, */*")
                setRequestProperty("Accept-Encoding", "gzip")
                setRequestProperty("User-Agent", "Timewrap/1.0 (Android)")
                instanceFollowRedirects = false
            }

            try {
                when (val code = connection.responseCode) {
                    in 200..299 -> return connection.read()

                    // `instanceFollowRedirects` ne suit pas HTTP vers HTTPS :
                    // on s'en charge, c'est justement le cas des vieux ENT.
                    301, 302, 303, 307, 308 -> {
                        val location = connection.getHeaderField("Location")
                            ?: error("redirection sans adresse")
                        url = URL(url, location)
                    }

                    401, 403 -> error(
                        "l'ENT refuse l'accès ($code) : l'adresse d'abonnement est " +
                            "peut-être personnelle et expirée",
                    )

                    404 -> error("adresse introuvable (404)")
                    else -> error("l'ENT a répondu $code")
                }
            } finally {
                connection.disconnect()
            }
        }

        error("trop de redirections")
    }

    private fun HttpURLConnection.read(): String {
        val gzipped = contentEncoding?.equals("gzip", ignoreCase = true) == true
        val stream = if (gzipped) GZIPInputStream(inputStream) else inputStream
        val bytes = stream.use { it.readBytes(MAX_BYTES) }
        val text = bytes.toString(Charsets.UTF_8)

        if (!text.contains("BEGIN:VCALENDAR")) {
            error(
                "la réponse n'est pas un calendrier — vérifiez l'adresse, " +
                    "c'est souvent une page de connexion qui arrive à la place",
            )
        }
        return text
    }

    /** Lit au plus `limit` octets, sans charger un flux sans fin en mémoire. */
    private fun java.io.InputStream.readBytes(limit: Int): ByteArray {
        val buffer = java.io.ByteArrayOutputStream()
        val chunk = ByteArray(16 * 1024)
        while (true) {
            val read = read(chunk)
            if (read <= 0) break
            buffer.write(chunk, 0, read)
            if (buffer.size() > limit) error("le fichier dépasse ${limit / (1024 * 1024)} Mo")
        }
        return buffer.toByteArray()
    }

    private fun normalize(raw: String): String {
        val trimmed = raw.trim()
        return when {
            trimmed.startsWith("webcal://", ignoreCase = true) ->
                "https://" + trimmed.removePrefix("webcal://").removePrefix("WEBCAL://")
            trimmed.startsWith("http://", ignoreCase = true) -> trimmed
            trimmed.startsWith("https://", ignoreCase = true) -> trimmed
            else -> "https://$trimmed"
        }
    }
}
