package io.github.elaizes.elamon

import java.io.File
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertNull
import kotlin.test.assertTrue
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.jsonObject
import io.github.elaizes.elamon.data.previewFrom

/** The editor's preview while the computer can't be asked for it: asked again less and less often, the last one marked old. */
class EditorPreviewTest {
	private val dir = File(System.getProperty("elamon.fixtures") ?: error("elamon.fixtures isn't set; run the tests through Gradle"))

	@Test
	fun thePreviewBacksOffWhileItFails() {
		// Shown: again every interval.
		assertEquals(1000L, previewWaitMs(0, 1000))
		assertEquals(250L, previewWaitMs(0, 250))
		// Failing, in a row: 1, 2, 4, 8 s and 8 s on, as a lost computer is looked for again.
		assertEquals(listOf(1000L, 2000L, 4000L, 8000L, 8000L, 8000L), (1..6).map { previewWaitMs(it, 250) })
		// Never sooner than the interval.
		assertEquals(listOf(5000L, 5000L, 5000L, 8000L), (1..4).map { previewWaitMs(it, 5000) })
		assertEquals(20_000L, previewWaitMs(3, 20_000))
	}

	@Test
	fun aFailedPreviewIsMarkedOld() {
		val answer = Json.parseToJsonElement(File(dir, "0.11/admin-preview.json").readText()).jsonObject.getValue("response").jsonObject
		val view = PreviewView(previewFrom(answer), emptyMap(), 1L)
		assertFalse(view.stale)
		val failed = previewFailed(EditorState(preview = view), Message.Text("out of reach"))
		// The same preview, its numbers old, and why.
		assertEquals(view.copy(stale = true), failed.preview)
		assertEquals(Message.Text("out of reach"), failed.previewNote)
		// None before, none after.
		val none = previewFailed(EditorState(), Message.Text("out of reach"))
		assertNull(none.preview)
		assertTrue(none.previewNote != null)
	}
}
