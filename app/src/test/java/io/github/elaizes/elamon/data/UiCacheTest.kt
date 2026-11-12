package io.github.elaizes.elamon.data

import java.io.File
import java.nio.file.Files
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertNull
import kotlin.test.assertTrue
import kotlinx.coroutines.test.runTest

/** The last ui of each computer, its readings and its profile, kept to start with and to show while offline. */
class UiCacheTest {
	private val fixtures = File(System.getProperty("elamon.fixtures") ?: error("elamon.fixtures isn't set; run the tests through Gradle"))
	private val ui = ProtocolJson.decodeFromString(UiDoc.serializer(), File(fixtures, "ui.json").readText())
	private val data = ProtocolJson.decodeFromString(StreamLine.serializer(), File(fixtures, "stream-data.json").readText()) as DataLine
	private val id = "0123456789abcdef0123456789abcdef"

	private fun dir(): File = Files.createTempDirectory("ui-cache").toFile().also { it.deleteOnExit() }

	@Test
	fun keptAndReadBack() = runTest {
		val d = dir()
		val cache = UiCache(d)
		assertNull(cache.ui(id))
		assertEquals(emptyMap(), cache.data(id))
		cache.putUi(id, ui)
		cache.putData(id, mapOf("home" to data))
		assertEquals(ui, cache.ui(id))
		// The readings without their history: the graphs ask for theirs again.
		assertEquals(data.copy(history = null), cache.data(id).getValue("home"))
		assertEquals(ui.profile, cache.profiles.value[id])
		// Another run reads the profiles at once, before anything is asked.
		assertEquals(mapOf(id to ui.profile), UiCache(d).profiles.value)
	}

	@Test
	fun aForgottenComputerLeavesNothing() = runTest {
		val d = dir()
		val cache = UiCache(d)
		cache.putUi(id, ui)
		cache.putData(id, mapOf("home" to data))
		cache.remove(id)
		assertNull(cache.ui(id))
		assertEquals(emptyMap(), cache.data(id))
		assertEquals(emptyMap(), cache.profiles.value)
		assertTrue(d.listFiles().orEmpty().isEmpty())
	}

	@Test
	fun whatNoLongerReadsIsAskedForAgain() = runTest {
		val d = dir()
		File(d, "$id.ui.json").writeText("{broken")
		File(d, "$id.profile.json").writeText("[]")
		val cache = UiCache(d)
		assertNull(cache.ui(id))
		assertEquals(emptyMap(), cache.profiles.value)
	}

	@Test
	fun onlyAServicesIdNamesAFile() = runTest {
		val d = dir()
		val cache = UiCache(d)
		cache.putUi("../escape", ui)
		assertNull(cache.ui("../escape"))
		assertTrue(d.listFiles().orEmpty().isEmpty())
		assertTrue(!File(d.parentFile, "escape.ui.json").exists())
	}
}
