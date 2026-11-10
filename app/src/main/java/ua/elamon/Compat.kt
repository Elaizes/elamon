package ua.elamon

import ua.elamon.data.APP_API

/** Whether this app and a computer's service understand each other fully. */
enum class Compat {
	Ok,

	/** The service speaks a newer API or asks for a newer app. */
	UpdateApp,

	/** The service is older than this app expects: some things are missing until it's updated. */
	UpdateElamond,
}

/** The service version whose fields this app reads; older ones work with fallbacks. */
const val MIN_ELAMOND = "0.4.0"

object Versions {
	/**
	 * Compares "0.4.0"-style versions by their numbers: "0.4" equals "0.4.0", a leading "v" and
	 * anything after the numbers ("-rc1", "+git") are ignored, and a version without numbers
	 * counts as 0.
	 */
	fun compare(a: String, b: String): Int {
		val x = numbers(a)
		val y = numbers(b)
		for (i in 0 until maxOf(x.size, y.size)) {
			val c = x.getOrElse(i) { 0 }.compareTo(y.getOrElse(i) { 0 })
			if (c != 0) return c
		}
		return 0
	}

	private fun numbers(v: String): List<Int> =
		v.trim().removePrefix("v").takeWhile { it.isDigit() || it == '.' }
			.split('.').filter { it.isNotEmpty() }.map { it.toIntOrNull() ?: Int.MAX_VALUE }
}

/** What to tell about this app ([appVersion]) and an service of [serviceVersion] speaking [serviceApi]. */
fun compatibility(appVersion: String, serviceApi: Int, serviceVersion: String, minApp: String?): Compat = when {
	serviceApi > APP_API -> Compat.UpdateApp
	minApp != null && Versions.compare(appVersion, minApp) < 0 -> Compat.UpdateApp
	Versions.compare(serviceVersion, MIN_ELAMOND) < 0 -> Compat.UpdateElamond
	else -> Compat.Ok
}
