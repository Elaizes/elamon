package io.github.elaizes.elamon

import java.io.File
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertNull
import io.github.elaizes.elamon.data.AndroidProfile
import io.github.elaizes.elamon.data.Machine
import io.github.elaizes.elamon.data.Profile
import io.github.elaizes.elamon.data.ProtocolJson
import io.github.elaizes.elamon.data.UiDoc
import io.github.elaizes.elamon.data.UnlockProfile

/** How the app looks and behaves, from the computers' profiles; and the settings 0.5 kept, as one. */
class SettingsTest {
	private val fixtures = File(System.getProperty("elamon.fixtures") ?: error("elamon.fixtures isn't set; run the tests through Gradle"))

	@Test
	fun theDefaultProfileIsTheAppsDefaults() {
		// Without a computer's profile, the app is as the built-in `default` profile says.
		assertEquals(Prefs(), Prefs.of(Profile()))
	}

	@Test
	fun theFixturesProfile() {
		val ui = ProtocolJson.decodeFromString(UiDoc.serializer(), File(fixtures, "ui.json").readText())
		val p = Prefs.of(ui.profile)
		assertEquals(ThemeMode.Dark, p.theme)
		assertEquals(true, p.pureBlack)
		assertEquals(false, p.dynamicColor)
		assertEquals(true, p.blur)
		assertEquals(UnlockGrace.OneMinute, p.unlockGrace)
		assertEquals(setOf(Guarded.Power, Guarded.Processes, Guarded.Actions, Guarded.Units), p.unlockFor)
		assertEquals(1000, p.intervalMs)
	}

	@Test
	fun whatAProfileSays() {
		val p = Prefs.of(
			Profile(
				theme = "light", animations = false, intervalMs = 100, fahrenheit = true, networkBits = true, confirmActions = false,
				unlock = UnlockProfile(listOf("media", "profile", "lights"), "5m"),
				android = AndroidProfile(dynamicColor = false, blur = false, haptics = false, keepScreenOn = true),
			),
		)
		assertEquals(ThemeMode.Light, p.theme)
		assertEquals(false, p.animations)
		// Never faster than a client may ask.
		assertEquals(Intervals.MIN_MS, p.intervalMs)
		assertEquals(true, p.fahrenheit)
		assertEquals(true, p.networkBits)
		assertEquals(false, p.confirmActions)
		// A kind a newer service names is left out.
		assertEquals(setOf(Guarded.Media, Guarded.Profile), p.unlockFor)
		assertEquals(UnlockGrace.FiveMinutes, p.unlockGrace)
		assertEquals(false, p.dynamicColor)
		assertEquals(false, p.blur)
		assertEquals(false, p.haptics)
		assertEquals(true, p.keepScreenOn)
		// A theme this app doesn't know follows the phone.
		assertEquals(ThemeMode.System, Prefs.of(Profile(theme = "sepia")).theme)
	}

	@Test
	fun theHomeScreenLooksAsTheFirstPairedComputer() {
		val a = Machine("a".repeat(32), "deskpc", listOf("192.0.2.1"), 47823, "f")
		val b = Machine("b".repeat(32), "laptop", listOf("192.0.2.2"), 47823, "f")
		val profiles = mapOf(a.id to Profile(theme = "dark"), b.id to Profile(theme = "light"))
		assertEquals(ThemeMode.Dark, prefsFor(Screen.Home, listOf(a, b), profiles).theme)
		// A computer's screen, and the settings opened from it, look as that computer says.
		assertEquals(ThemeMode.Light, prefsFor(Screen.Machine(b.id), listOf(a, b), profiles).theme)
		assertEquals(ThemeMode.Light, prefsFor(Screen.Settings(b.id), listOf(a, b), profiles).theme)
		assertEquals(ThemeMode.Dark, prefsFor(Screen.Settings(), listOf(a, b), profiles).theme)
		// One never reached yet, or none paired: the defaults.
		assertEquals(Prefs(), prefsFor(Screen.Home, listOf(b, a), mapOf(a.id to Profile(theme = "dark"))))
		assertEquals(Prefs(), prefsFor(Screen.Home, emptyList(), profiles))
	}

	@Test
	fun noSettingsNothingToCarryOver() {
		assertNull(Migration.toml(emptyMap<String, Any>()))
		// Only the state of a section, or everything as it was by default.
		assertNull(Migration.toml(mapOf("displays_open" to true)))
		val asDefault = mapOf(
			"theme" to "System", "pure_black" to false, "dynamic_color" to true, "blur" to true, "animations" to true,
			"interval_ms" to 1000, "keep_screen_on" to false, "fahrenheit" to false, "network_bits" to false,
			"confirm_actions" to true, "unlock_grace" to "EveryTime", "haptics" to true,
			"unlock_power" to true, "unlock_processes" to true, "unlock_actions" to true, "unlock_units" to true,
			"unlock_profile" to false, "unlock_media" to false, "unlock_transfer" to true,
		)
		assertNull(Migration.toml(asDefault))
	}

	@Test
	fun theSettingsOf05AsAProfile() {
		// As 0.5 wrote them once anything was changed: every key.
		val old = mapOf(
			"theme" to "Dark", "pure_black" to true, "dynamic_color" to false, "blur" to true, "animations" to true,
			"interval_ms" to 2000, "keep_screen_on" to true, "fahrenheit" to true, "network_bits" to false,
			"confirm_actions" to true, "unlock_grace" to "OneMinute", "haptics" to true,
			"unlock_power" to true, "unlock_processes" to true, "unlock_actions" to true, "unlock_units" to false,
			"unlock_profile" to false, "unlock_media" to true, "unlock_transfer" to true, "displays_open" to true,
		)
		val expected = """
			[profiles.phone]
			use = "default"
			theme = "dark"
			pure_black = true
			interval_ms = 2000
			fahrenheit = true
			unlock = { for = ["power", "signal", "actions", "media"], grace = "1m" }

			[profiles.phone.android]
			dynamic_color = false
			keep_screen_on = true

		""".trimIndent()
		assertEquals(expected, Migration.toml(old))
	}

	@Test
	fun unlockingAsBeforeWhereItWasntChanged() {
		// 0.5 asked for power, processes and actions by default, not units: that differs from 0.6's default.
		val toml = Migration.toml(mapOf("unlock_grace" to "EveryTime", "unlock_media" to false))!!
		assertEquals("[profiles.phone]\nuse = \"default\"\nunlock = { for = [\"power\", \"signal\", \"actions\"], grace = \"every_time\" }\n", toml)
		// Only the Android part differs: no common keys but use.
		assertEquals("[profiles.phone]\nuse = \"default\"\n\n[profiles.phone.android]\nblur = false\n", Migration.toml(mapOf("blur" to false, "unlock_units" to true)))
	}
}
