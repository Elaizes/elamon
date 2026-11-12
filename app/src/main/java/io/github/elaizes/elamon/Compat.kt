package io.github.elaizes.elamon

import io.github.elaizes.elamon.data.APP_API

/** Whether this app and the Elamon service on a computer understand each other. */
enum class Compat {
	Ok,

	/** The service speaks a newer API or asks for a newer app. */
	UpdateApp,

	/** The service is older than this app works with (API 1, elamond 0.5 and before): it needs updating. */
	UpdateElamond,
}

/** The oldest elamond this app works with: the first with protocol API 2. */
const val MIN_ELAMOND = "0.6.0"

object Versions {
	/**
	 * Compares "1.2.0"-style versions by their numbers: "1.2" equals "1.2.0", a leading "v" and
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

/**
 * What to tell about this app ([appVersion]) and elamond [elamondVersion] speaking [elamondApi]:
 * nothing but API 2 is served or read, so any other API means one of the two is to be updated, and
 * there are no dashboards to show until it is.
 */
fun compatibility(appVersion: String, elamondApi: Int, elamondVersion: String, minApp: String?): Compat = when {
	elamondApi > APP_API -> Compat.UpdateApp
	elamondApi < APP_API -> Compat.UpdateElamond
	minApp != null && Versions.compare(appVersion, minApp) < 0 -> Compat.UpdateApp
	Versions.compare(elamondVersion, MIN_ELAMOND) < 0 -> Compat.UpdateElamond
	else -> Compat.Ok
}
