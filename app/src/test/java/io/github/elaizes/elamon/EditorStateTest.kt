package io.github.elaizes.elamon

import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertNull
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.jsonObject
import io.github.elaizes.elamon.data.Mistake

/** The editor's state as the draft changes and as what it opens is read again. */
class EditorStateTest {
	private fun obj(text: String): JsonObject = Json.parseToJsonElement(text).jsonObject

	@Test
	fun aWritesMistakesGoOnceTheDraftChanges() {
		val wrong = Mistake(file = "elamond.d/zz-editor.toml", line = 3, path = "dashboards.mine.widgets[0].max", message = "should be a number")
		val preview = Mistake(path = "dashboards.mine.title", message = "is needed")
		val s = EditorState(draft = obj("""{"title":"Mine"}"""), mistakes = listOf(wrong), previewMistakes = listOf(preview))
		val fixed = edited(s, obj("""{"title":"Mine","widgets":[]}"""))
		assertEquals(obj("""{"title":"Mine","widgets":[]}"""), fixed.draft)
		assertEquals(emptyList(), fixed.mistakes)
		// The preview's are the preview's own, until it finds none.
		assertEquals(listOf(preview), fixed.previewMistakes)
	}

	@Test
	fun whatIsGoneFromTheSettingsIsSaid() {
		val why = Message.Text("“mine” isn't in this computer's settings")
		// Nothing open yet: the screen says it.
		val (first, said) = notInSettings(EditorState(loading = false), why)
		assertEquals(why, first.failed)
		assertNull(said)
		// Read again with something open: it stays as it was, and is said.
		val open = EditorState(loading = false, editing = Editing.Board("mine"), draft = obj("""{"title":"Mine"}"""), base = obj("""{"title":"Mine"}"""))
		val (again, told) = notInSettings(open, why)
		assertEquals(open, again)
		assertEquals(why, told)
	}
}
