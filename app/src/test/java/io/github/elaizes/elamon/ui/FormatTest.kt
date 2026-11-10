package io.github.elaizes.elamon.ui

import java.util.Locale
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertTrue

class FormatTest {
	@Test
	fun bytes() {
		assertEquals("0 B", Fmt.bytes(0L))
		assertEquals("1023 B", Fmt.bytes(1023L))
		assertEquals("1.00 KiB", Fmt.bytes(1024L))
		assertEquals("1.50 KiB", Fmt.bytes(1536L))
		assertEquals("16.0 GiB", Fmt.bytes(16L shl 30))
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
	fun bytesParts() {
		assertEquals("512" to "MiB", Fmt.bytesParts(512.0 * 1024 * 1024))
	}

	@Test
	fun rates() {
		assertEquals("1.00 KiB/s", Fmt.rate(1024.0))
		assertEquals("0 bit/s", Fmt.rate(0.0, bits = true))
		assertEquals("800 bit/s", Fmt.rate(100.0, bits = true))
		assertEquals("10.0 Mbit/s", Fmt.rate(1_250_000.0, bits = true))
		assertEquals("Mbit/s", Fmt.rateParts(1_250_000.0, bits = true).second)
	}

	@Test
	fun ratesJustBelowABoundary() {
		assertEquals("1.00 kbit/s", Fmt.rate(124.96, bits = true))
		assertEquals("1.00 Mbit/s", Fmt.rate(124_996.0, bits = true))
		assertEquals("1.00 MiB/s", Fmt.rate(1024.0 * 1024 - 10))
	}

	@Test
	fun percentAndTemperature() {
		assertEquals("100%", Fmt.percent(99.6f))
		assertEquals("12.5%", Fmt.percent(12.5f, 1))
		assertEquals("37°C", Fmt.temp(36.6f))
		assertEquals("212°F", Fmt.temp(100f, fahrenheit = true))
		assertEquals("32°F", Fmt.temp(0f, fahrenheit = true))
	}

	@Test
	fun clocksAndFrequencies() {
		assertEquals("999 MHz", Fmt.mhz(999))
		assertEquals("1.00 GHz", Fmt.mhz(1000))
		assertEquals("3.10 GHz", Fmt.mhz(3100))
		assertEquals("60 Hz", Fmt.hz(59.96f))
		assertEquals("59.94 Hz", Fmt.hz(59.94f))
		assertEquals("144 Hz", Fmt.hz(144f))
	}

	@Test
	fun times() {
		assertEquals("0:00", Fmt.clock(0))
		assertEquals("3:07", Fmt.clock(187_000_000))
		assertEquals("1:02:45", Fmt.clock(3_765_000_000))
		assertEquals("1", Fmt.seconds(1000))
		assertEquals("0.25", Fmt.seconds(250))
		assertEquals("1.5", Fmt.seconds(1500))
		assertEquals("60", Fmt.seconds(60_000))
	}

	@Test
	fun other() {
		assertEquals("12.3 W", Fmt.watts(12.34f))
		assertEquals("24.0″", Fmt.inches(530, 300))
		assertEquals("53 × 30 cm", Fmt.centimetres(530, 300))
		assertEquals("2500 RPM", Fmt.rpm(2500))
		assertEquals("1000 Mbit/s", Fmt.linkSpeed(1000))
	}

	@Test
	fun sameDecimalPointWhateverTheLanguage() {
		val before = Locale.getDefault()
		try {
			Locale.setDefault(Locale.GERMANY)
			// Next to figures that always use a point, a comma here would read as a different kind of number.
			assertEquals("52.4 Wh", Fmt.wattHours(52.4f))
			assertEquals("0.52", Fmt.load(0.523f))
			assertEquals("12.3 W", Fmt.watts(12.34f))
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
