package ua.elamon

import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.awaitCancellation
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch

/**
 * What [block] returns, or null when it fails. Cancellation is passed on, never taken for a
 * failure: a screen that has gone must stop its work, not record "offline".
 */
inline fun <T> orNull(block: () -> T): T? = try {
	block()
} catch (e: CancellationException) {
	throw e
} catch (e: Exception) {
	null
}

/**
 * Asks about each of [items] in a loop of its own until cancelled: one request at a time, the next
 * [everyMs] after the last one ended, so a slow answer is never overtaken by a newer one.
 */
suspend fun <T, R> pollEach(items: List<T>, everyMs: Long, probe: suspend (T) -> R, report: (T, R) -> Unit): Nothing = coroutineScope {
	for (item in items) {
		launch {
			while (true) {
				report(item, probe(item))
				delay(everyMs)
			}
		}
	}
	awaitCancellation()
}
