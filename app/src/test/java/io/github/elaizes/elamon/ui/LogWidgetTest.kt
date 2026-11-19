package io.github.elaizes.elamon.ui

import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertTrue
import io.github.elaizes.elamon.R
import io.github.elaizes.elamon.data.Widget

/** A log's card: what it says of itself, a journal's or the events'. */
class LogWidgetTest {
	private val events = Widget("events/events", "log", scope = "events")
	private val journal = Widget("journal/system", "log", scope = "system")

	@Test
	fun theEventsAreSearchedAsEvents() {
		// A106: the events' search field said "Search the journal".
		assertTrue(ofEvents(events))
		assertEquals(R.string.log_search_events, searchHint(events))
		assertFalse(ofEvents(journal))
		assertEquals(R.string.log_search, searchHint(journal))
		assertEquals(R.string.log_search, searchHint(Widget("sources/app_log", "log", scope = "unit")))
	}
}
