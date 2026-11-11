package io.github.elaizes.elamon.ui

import io.github.elaizes.elamon.Message
import io.github.elaizes.elamon.R

/** How long ago, in the largest whole unit: "now", "30 s ago", "4 min ago", "2 h ago", "3 d ago". */
fun agoWords(seconds: Long): Words = when {
	seconds < AGO_NOW_S -> Words.Str(R.string.ago_now)
	seconds < SECONDS_PER_MINUTE -> Words.Count(R.plurals.ago_seconds, seconds.toInt())
	seconds < SECONDS_PER_HOUR -> Words.Count(R.plurals.ago_minutes, (seconds / SECONDS_PER_MINUTE).toInt())
	seconds < SECONDS_PER_DAY -> Words.Count(R.plurals.ago_hours, (seconds / SECONDS_PER_HOUR).toInt())
	else -> Words.Count(R.plurals.ago_days, (seconds / SECONDS_PER_DAY).toInt())
}

/** Under this, it just happened. */
private const val AGO_NOW_S = 2L
private const val SECONDS_PER_MINUTE = 60L
private const val SECONDS_PER_HOUR = 3_600L
private const val SECONDS_PER_DAY = 86_400L

/** How a request in the activity log ended, as a word or two for its tag. */
data class LogResult(
	val words: Message.Res,
	/** Let through. */
	val ok: Boolean,
	/** Turned away before anything was tried: the role, control, too often, the system's rules. */
	val refused: Boolean,
)

private val REFUSED = mapOf(
	"not_allowed" to R.string.log_not_allowed,
	"control_disabled" to R.string.log_control_off,
	"too_many_requests" to R.string.log_too_often,
	"not_permitted" to R.string.log_system_refused,
	"protected_process" to R.string.log_system_refused,
)

private val FAILED = mapOf(
	"failed" to R.string.log_failed,
	"process_changed" to R.string.log_process_changed,
	"storage" to R.string.log_not_saved,
	"not_found" to R.string.log_gone,
	"no_such_process" to R.string.log_gone,
	"no_such_action" to R.string.log_gone,
	"no_such_link" to R.string.log_gone,
	"no_such_service" to R.string.log_gone,
	"no_art" to R.string.log_gone,
	"unknown_signal" to R.string.log_invalid,
	"empty_command" to R.string.log_invalid,
	"no_unit" to R.string.log_invalid,
	"bad_name" to R.string.log_invalid,
	"bad_url" to R.string.log_invalid,
	"bad_unit" to R.string.log_invalid,
	"bad_scope" to R.string.log_invalid,
)

/** "ok" or the error code a log entry ended with; a code from a newer service is shown as it is. */
fun logResult(result: String): LogResult = when (result) {
	"ok" -> LogResult(Message.Res(R.string.log_ok), ok = true, refused = false)
	in REFUSED -> LogResult(Message.Res(REFUSED.getValue(result)), ok = false, refused = true)
	in FAILED -> LogResult(Message.Res(FAILED.getValue(result)), ok = false, refused = false)
	else -> LogResult(Message.Res(R.string.log_other, listOf(result)), ok = false, refused = false)
}
