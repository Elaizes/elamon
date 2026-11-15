package io.github.elaizes.elamon.data

import java.io.File
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import io.github.elaizes.elamon.allWidgets

/** What the service sends, read as the app's types; a broken answer is a protocol error. */
class DecodeTest {
	private val fixtures = File(System.getProperty("elamon.fixtures") ?: error("elamon.fixtures isn't set; run the tests through Gradle"))

	/** A ui whose one dashboard has [groups] groups inside each other. */
	private fun nested(groups: Int): String = buildString {
		append("""{"version":"1","service":{"id":"0123456789abcdef0123456789abcdef"},"tabs":["d"],"dashboards":[{"id":"d","widgets":[""")
		repeat(groups) { append("""{"id":"g$it","type":"group","widgets":[""") }
		append("""{"id":"v","type":"value"}""")
		repeat(groups) { append("]}") }
		append("]}]}")
	}

	@Test
	fun groupsInGroupsRead() {
		val ui = decodeChecked(UiDoc.serializer(), nested(25))
		assertEquals(26, allWidgets(ui.dashboards.single().widgets).size)
	}

	@Test
	fun brokenIsAProtocolError() {
		for (text in listOf("{", """{"version":1}""", "[]", "")) {
			val e = assertFailsWith<ElamondException> { decodeChecked(UiDoc.serializer(), text) }
			assertEquals(ElamondException.Kind.Protocol, e.kind)
		}
	}

	@Test
	fun theFixturesRead() {
		for (name in listOf("ui", "ui-default")) {
			decodeChecked(UiDoc.serializer(), File(fixtures, "$name.json").readText())
		}
		decodeChecked(DataLine.serializer(), File(fixtures, "stream-data.json").readText())
	}
}
