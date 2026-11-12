package io.github.elaizes.elamon.ui

import java.io.File
import java.util.Locale
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertNull
import kotlin.test.assertTrue
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive

class FormatTest {
	private val fixtures = File(System.getProperty("elamon.fixtures") ?: error("elamon.fixtures isn't set; run the tests through Gradle"))

	/** elamond/fixtures/formats.json: what the service writes into strings, the app writes the same. */
	@Test
	fun everyCaseTheServiceIsTestedAgainst() {
		val cases = Json.parseToJsonElement(File(fixtures, "formats.json").readText()).jsonArray
		assertTrue(cases.isNotEmpty())
		val wrong = cases.mapNotNull { c ->
			val o = c.jsonObject
			val format = o.getValue("format").jsonPrimitive.content
			val unit = o.getValue("unit").jsonPrimitive.content
			val expected = o.getValue("text").jsonPrimitive.content
			val got = Fmt.format(o.getValue("value"), format, unit)
			if (got == expected) null else "$format $unit ${o["value"]}: \"$got\", not \"$expected\""
		}
		assertTrue(wrong.isEmpty(), wrong.joinToString("\n"))
	}

	@Test
	fun halvesRoundAwayFromZero() {
		assertEquals("13%", Fmt.format(JsonPrimitive(12.5), "percent", "%"))
		assertEquals("-13%", Fmt.format(JsonPrimitive(-12.5), "percent", "%"))
		assertEquals("3", Fmt.round(2.5, 0))
		assertEquals("0.13", Fmt.round(0.125, 2))
		// Nothing left reads without a sign.
		assertEquals("0", Fmt.round(-0.4, 0))
	}

	@Test
	fun bytesJustBelowABoundary() {
		// Rounding mustn't show "1024 B", "10.00 KiB" or "100.0 KiB".
		assertEquals("1.00 KiB", Fmt.bytes(1023.6))
		assertEquals("10.0 KiB", Fmt.bytes(9.999 * 1024))
		assertEquals("100 KiB", Fmt.bytes(99.97 * 1024))
		assertEquals("1.00 MiB", Fmt.bytes(1023.7 * 1024))
		assertEquals("1.00 GiB", Fmt.bytes(1024.0 * 1024 * 1024 - 1))
		assertEquals("9.99 KiB", Fmt.bytes(9.99 * 1024))
		assertEquals("1023 KiB", Fmt.bytes(1023.4 * 1024))
	}

	@Test
	fun partsForLargeFigures() {
		assertEquals("512" to "MiB", Fmt.parts(512.0 * 1024 * 1024, "bytes", "B"))
		assertEquals("42" to "%", Fmt.parts(42.4, "percent", "%"))
		assertEquals("56" to "°C", Fmt.parts(55.5, "temp", "°C"))
		assertEquals("132" to "°F", Fmt.parts(131.9, "temp", "°F"))
		assertEquals("1.47" to "KiB/s", Fmt.parts(1500.5, "rate", "B/s"))
		assertEquals("12.0" to "Mbit/s", Fmt.parts(12_000_000.0, "rate", "bit/s"))
		assertEquals("3.10" to "GHz", Fmt.parts(3100.0, "mhz", "MHz"))
		assertEquals("4 h 12 min" to null, Fmt.parts(15120.0, "duration", "s"))
		assertEquals("12.3" to "W", Fmt.parts(12.25, null, "W"))
	}

	@Test
	fun whatIsNotANumber() {
		assertNull(Fmt.format(null, "percent", "%"))
		assertNull(Fmt.format(JsonNull, "bytes", "B"))
		assertEquals("On battery", Fmt.format(JsonPrimitive("On battery"), "percent", "%"))
		assertEquals("yes", Fmt.format(JsonPrimitive(true), null, null))
		assertEquals("no", Fmt.format(JsonPrimitive(false), "text", null))
		// No format: up to one decimal, with the unit.
		assertEquals("12.5 W", Fmt.format(JsonPrimitive(12.5), null, "W"))
		assertEquals("7", Fmt.format(JsonPrimitive(7), null, null))
	}

	@Test
	fun timesFromSecondsOrTheLogsMilliseconds() {
		assertEquals(Fmt.time(1_700_000_100.0, millis = false), Fmt.format(JsonPrimitive(1_700_000_100), "time", "s", "start"))
		assertEquals(Fmt.time(1_700_000_100_000.0, millis = true), Fmt.format(JsonPrimitive(1_700_000_100_000), "time", null, "t"))
		assertEquals(Fmt.time(1_700_000_100.0, millis = false), Fmt.time(1_700_000_100_000.0, millis = true))
	}

	@Test
	fun clocksAndSeconds() {
		assertEquals("0:00", Fmt.clock(0.0))
		assertEquals("3:07", Fmt.clock(187.4))
		assertEquals("1:02:45", Fmt.clock(3765.0))
		assertEquals("1", Fmt.seconds(1000))
		assertEquals("0.25", Fmt.seconds(250))
		assertEquals("1.5", Fmt.seconds(1500))
		assertEquals("60", Fmt.seconds(60_000))
	}

	@Test
	fun sameDecimalPointWhateverTheLanguage() {
		val before = Locale.getDefault()
		try {
			Locale.setDefault(Locale.GERMANY)
			// Next to figures that always use a point, a comma here would read as a different kind of number.
			assertEquals("52.4 Wh", Fmt.number(52.4, "wh", "Wh"))
			assertEquals("0.52", Fmt.number(0.523, "load", ""))
			assertEquals("12.3 W", Fmt.number(12.34, "watts", "W"))
			assertEquals("1.50 KiB", Fmt.number(1536.0, "bytes", "B"))
		} finally {
			Locale.setDefault(before)
		}
	}

	@Test
	fun avatarHues() {
		// Its hash is Int.MIN_VALUE, whose abs() is negative: Color.hsl threw on it.
		assertEquals(Int.MIN_VALUE, "polygenelubricants".hashCode())
		val hue = avatarHue("polygenelubricants")
		assertTrue(hue >= 0f && hue < 360f)
		assertEquals(avatarHue("Firefox"), avatarHue("firefox"))
		for (name in listOf("", "a", "systemd", "kworker/0:1")) assertTrue(avatarHue(name) in 0f..359f, name)
	}
}
