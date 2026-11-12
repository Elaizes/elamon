package io.github.elaizes.elamon.data

import java.io.File
import java.nio.file.Files
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertNull
import kotlin.test.assertTrue
import kotlinx.coroutines.test.runTest
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive

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
		// The readings without their history (the graphs ask for theirs again), nor the links' rows and what plays.
		val kept = cache.data(id).getValue("home")
		assertEquals(data.copy(history = null, widgets = data.widgets - "home/links" - "home/media"), kept)
		assertEquals(ui.profile, cache.profiles.value[id])
		// Another run reads the profiles at once, before anything is asked.
		assertEquals(mapOf(id to ui.profile), UiCache(d).profiles.value)
	}

	@Test
	fun figuresAreKeptNotWhatIsDoneOnTheComputer() {
		val detail = DataLine(
			t = 1,
			dashboard = "detail",
			widgets = mapOf(
				"detail/procs" to WidgetState(rows = listOf(Row("4242", f = JsonObject(mapOf("cmd" to JsonPrimitive("secret --token x"))))), total = 1),
				"detail/cpu" to WidgetState(v = JsonPrimitive(12.5)),
			),
		)
		val readings = mapOf("home" to data, "detail" to detail)
		val kept = UiCache.keptReadings(ui, readings)
		// Processes, links and the media player's titles go; figures, and the disks' rows, stay.
		assertEquals(setOf("detail/cpu"), kept.getValue("detail").widgets.keys)
		assertEquals(data.widgets.keys - "home/links" - "home/media", kept.getValue("home").widgets.keys)
		assertEquals(data.widgets["home/disks"], kept.getValue("home").widgets["home/disks"])
		// Without the ui to tell which tables are which: no table's rows and no player.
		val blind = UiCache.keptReadings(null, readings)
		assertEquals(setOf("detail/cpu"), blind.getValue("detail").widgets.keys)
		assertEquals(data.widgets.keys - "home/links" - "home/media" - "home/disks", blind.getValue("home").widgets.keys)
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
	fun anAnswerFromBeforeTheComputerWasForgottenIsntKept() = runTest {
		val d = dir()
		val cache = UiCache(d)
		// Asked before Forget computer, answered after it: the screen's ui, the readings it kept going away.
		val before = cache.ticket(id)
		cache.remove(id)
		cache.putUi(id, ui, before)
		cache.putData(id, mapOf("home" to data), before)
		assertTrue(d.listFiles().orEmpty().isEmpty())
		assertNull(cache.ui(id))
		assertEquals(emptyMap(), cache.profiles.value)
		// Paired again: what is asked from now on is kept again.
		cache.putUi(id, ui, cache.ticket(id))
		assertEquals(ui, cache.ui(id))
	}

	@Test
	fun aUiIsReadFromItsFileOnce() = runTest {
		val d = dir()
		UiCache(d).putUi(id, ui)
		val cache = UiCache(d)
		val first = cache.ui(id)
		assertEquals(ui, first)
		// Every look after the first is the document already read, not the file decoded again.
		File(d, "$id.ui.json").writeText("{broken")
		assertTrue(first === cache.ui(id))
		cache.remove(id)
		assertNull(cache.ui(id))
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
