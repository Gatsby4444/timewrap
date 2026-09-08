package app.timewrap.ui

import app.timewrap.core.Occurrence
import java.time.Instant
import java.time.LocalDate
import java.time.LocalTime
import java.time.ZoneId
import java.time.format.DateTimeFormatter
import java.time.format.TextStyle
import java.util.Locale

/** Le fuseau dans lequel l'application raisonne : celui de l'appareil. */
val deviceZone: ZoneId get() = ZoneId.systemDefault()

private val hourMinute = DateTimeFormatter.ofPattern("HH:mm")

fun Long.toInstantUtc(): Instant = Instant.ofEpochSecond(this)

fun Long.toLocalTime(zone: ZoneId = deviceZone): LocalTime =
    toInstantUtc().atZone(zone).toLocalTime()

fun Long.toLocalDate(zone: ZoneId = deviceZone): LocalDate =
    toInstantUtc().atZone(zone).toLocalDate()

fun Long.formatTime(zone: ZoneId = deviceZone): String = toLocalTime(zone).format(hourMinute)

fun todayEpochDay(zone: ZoneId = deviceZone): Long = LocalDate.now(zone).toEpochDay()

/** « lun. 21 sept. » — assez court pour un onglet, assez précis pour se repérer. */
fun LocalDate.shortLabel(locale: Locale = Locale.getDefault()): String {
    val day = dayOfWeek.getDisplayName(TextStyle.SHORT, locale)
    val month = month.getDisplayName(TextStyle.SHORT, locale)
    return "$day $dayOfMonth $month"
}

/** « lundi 21 septembre » — pour un en-tête de journée. */
fun LocalDate.longLabel(locale: Locale = Locale.getDefault()): String {
    val day = dayOfWeek.getDisplayName(TextStyle.FULL, locale)
    val month = month.getDisplayName(TextStyle.FULL, locale)
    return "${day.replaceFirstChar { it.uppercase(locale) }} $dayOfMonth $month"
}

/** Le lundi de la semaine contenant ce jour. */
fun Long.startOfWeek(): Long {
    val date = LocalDate.ofEpochDay(this)
    return date.minusDays((date.dayOfWeek.value - 1).toLong()).toEpochDay()
}

/**
 * Une durée en langage courant : « 45 min », « 1 h 30 », « 2 j ».
 * Les emplois du temps se lisent en coup d'œil ; « 90 minutes » fait réfléchir.
 */
fun formatDuration(minutes: Long): String = when {
    minutes < 1 -> "moins d'une minute"
    minutes < 60 -> "$minutes min"
    minutes < 60 * 24 -> {
        val h = minutes / 60
        val m = minutes % 60
        if (m == 0L) "$h h" else "$h h $m"
    }
    else -> {
        val d = minutes / (60 * 24)
        if (d <= 1L) "1 jour" else "$d jours"
    }
}

/** « 08:00 – 10:00 », ou « toute la journée ». */
fun Occurrence.timeRange(zone: ZoneId = deviceZone): String =
    if (allDay) "Toute la journée" else "${startUtc.formatTime(zone)} – ${endUtc.formatTime(zone)}"

fun Occurrence.durationMinutes(): Long = (endUtc - startUtc) / 60
