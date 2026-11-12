package io.github.elaizes.elamon.data

import java.io.File
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlinx.serialization.SerializationException
import io.github.elaizes.elamon.allWidgets

/** What the service sends, read without letting a broken or hostile answer take the app down. */
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
	fun depthCountsObjectsAndArraysNotStrings() {
		assertEquals(0, jsonDepth(""))
		assertEquals(0, jsonDepth("42"))
		assertEquals(1, jsonDepth("{}"))
		assertEquals(3, jsonDepth("""[[{"a":1}],[]]"""))
		assertEquals(1, jsonDepth("""{"a":"[[[{{"}"""))
		// An escaped quote doesn't end the string, an escaped backslash before one does.
		assertEquals(1, jsonDepth("""{"a":"\"[[["}"""))
		assertEquals(2, jsonDepth("""{"a":"\\","b":[1]}"""))
		// It stops counting once past what it was asked about.
		assertEquals(4, jsonDepth("[[[[[[[[]]]]]]]]", stopAt = 3))
	}

	@Test
	fun groupsInGroupsAsFarAsAnyoneWrites() {
		val ui = decodeChecked(UiDoc.serializer(), nested(25))
		assertEquals(26, allWidgets(ui.dashboards.single().widgets).size)
	}

	@Test(timeout = 10_000)
	fun nestedPastAnyServiceIsAProtocolError() {
		// Read as it was, this overflowed the stack: an Error no screen catches, on every poll.
		for (text in listOf(nested(40), nested(200_000), "[".repeat(1_000_000))) {
			val e = assertFailsWith<ElamondException> { decodeChecked(UiDoc.serializer(), text) }
			assertEquals(ElamondException.Kind.Protocol, e.kind)
		}
		assertFailsWith<SerializationException> { decodeStreamLine("""{"type":"data","t":1,"dashboard":"d","widgets":{"w":{"v":""" + "[".repeat(100) + "]".repeat(100) + "}}}") }
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
