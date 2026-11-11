package io.github.elaizes.elamon.ui

import android.content.res.Resources
import androidx.annotation.PluralsRes
import androidx.annotation.StringRes
import io.github.elaizes.elamon.R
import io.github.elaizes.elamon.data.Display
import io.github.elaizes.elamon.data.Mode

/** Words still to be looked up in the resources: a string, or a plural and the number it counts. */
sealed interface Words {
	data class Str(@StringRes val id: Int) : Words
	data class Count(@PluralsRes val id: Int, val n: Int) : Words
}

/** The words, looked up. */
fun Words.text(res: Resources): String = when (this) {
	is Words.Str -> res.getString(id)
	is Words.Count -> res.getQuantityString(id, n, n)
}

/**
 * What the Displays section says while closed, in parts to set between dots: how many monitors
 * are connected and how many of those are on ("2 connected · 1 on"), or, with none, that none
 * are. Free ports aren't monitors and don't count.
 */
fun displaysSummary(displays: List<Display>): List<Words> {
	val connected = displays.filter { it.connected }
	if (connected.isEmpty()) return listOf(Words.Str(R.string.displays_none))
	return listOf(
		Words.Count(R.plurals.displays_count_connected, connected.size),
		Words.Count(R.plurals.displays_count_on, connected.count { it.active }),
	)
}

/** A monitor's maker and model, or its port when it doesn't say. */
fun displayName(d: Display): String =
	listOfNotNull(d.vendor, d.model).filter { it.isNotBlank() }.joinToString(" ").ifEmpty { d.connector }

/** "1920 × 1080 · 180 Hz". */
fun modeText(m: Mode): String = "${m.width} × ${m.height} · ${Fmt.hz(m.refresh)}"
