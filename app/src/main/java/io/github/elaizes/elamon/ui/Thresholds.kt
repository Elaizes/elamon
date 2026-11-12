package io.github.elaizes.elamon.ui

/**
 * How the bars and charts are scaled. Where a reading turns amber and red is the service's to say
 * (docs/config.md, "Levels"): it sends each value's level and how far it has turned toward the next.
 */
object Thresholds {
	/** The top of a percentage scale. */
	const val PERCENT = 100f

	/** Room above the highest point, so a line doesn't run along the top. */
	const val CHART_HEADROOM = 1.15f

	/** Rows the dashed grid divides a chart into. */
	const val CHART_GRID_ROWS = 4
}

/** Where a reading sits on the way from fine to too much. */
enum class Band { Good, GoodToWarn, Warn, WarnToHot, Hot }

/** A [band], and for the two that blend, how far across it the reading is, from 0 to 1. */
data class Level(val band: Band, val t: Float = 0f)

/**
 * The service's [level] ("normal", "warning", "critical") and [blend] (how far it has turned
 * toward the next, 0 to 1) as a colour band: normal turning is green to amber, warning turning is
 * amber to red. Null for no level, or one this app doesn't know.
 */
fun levelOf(level: String?, blend: Double?): Level? {
	val b = (blend ?: 0.0).toFloat().coerceIn(0f, 1f)
	return when (level) {
		"normal" -> if (b > 0f) Level(Band.GoodToWarn, b) else Level(Band.Good)
		"warning" -> if (b > 0f) Level(Band.WarnToHot, b) else Level(Band.Warn)
		"critical" -> Level(Band.Hot)
		else -> null
	}
}
