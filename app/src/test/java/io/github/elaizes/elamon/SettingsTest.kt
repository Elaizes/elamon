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
	fun thePhonesOwnThemeOverTheProfiles() {
		val a = Machine("a".repeat(32), "deskpc", listOf("192.0.2.1"), 47823, "f")
		val b = Machine("b".repeat(32), "laptop", listOf("192.0.2.2"), 47823, "f")
		val light = Profile(theme = "light", pureBlack = false, animations = false, intervalMs = 5000, confirmActions = false, android = AndroidProfile(dynamicColor = true))
		val profiles = mapOf(a.id to light, b.id to Profile(theme = "system", pureBlack = true, android = AndroidProfile(dynamicColor = false)))
		val own = OwnTheme(ThemeMode.Dark, pureBlack = true, dynamicColor = false)
		// Off: the profile's, as before.
		assertEquals(Prefs.of(light), prefsFor(Screen.Machine(a.id), listOf(a, b), profiles, own = null))
		// On: the phone's theme; the rest the profile's.
		val on = prefsFor(Screen.Machine(a.id), listOf(a, b), profiles, own)
		assertEquals(own, OwnTheme.of(on))
		assertEquals(Prefs.of(light).copy(theme = ThemeMode.Dark, pureBlack = true, dynamicColor = false), on)
		// The same theme at home, on any computer, in the settings, and with none paired.
		for (top in listOf(Screen.Home, Screen.Machine(b.id), Screen.Settings(), Screen.Settings(a.id))) {
			assertEquals(own, OwnTheme.of(prefsFor(top, listOf(a, b), profiles, own)), "$top")
		}
		assertEquals(own, OwnTheme.of(prefsFor(Screen.Home, emptyList(), emptyMap(), own)))
		// The profile changes while it is on: the theme doesn't, the rest follows.
		val changed = mapOf(a.id to light.copy(theme = "light", pureBlack = false, animations = true, android = AndroidProfile(dynamicColor = true)))
		val after = prefsFor(Screen.Machine(a.id), listOf(a), changed, own)
		assertEquals(own, OwnTheme.of(after))
		assertEquals(true, after.animations)
	}

	@Test
	fun thePhonesOwnThemeStartsAsTheThemeInForce() {
		val profile = Prefs.of(Profile(theme = "light", pureBlack = true, android = AndroidProfile(dynamicColor = false)))
		// Turned on the first time: what the profile gives now, so nothing jumps.
		val on = Look().turned(true, profile)
		assertEquals(OwnTheme(ThemeMode.Light, pureBlack = true, dynamicColor = false), on.theme)
		// Off: none over the profiles, but kept for next time, whatever the profile is by then.
		val off = on.copy(own = on.own!!.copy(theme = ThemeMode.Dark)).turned(false, profile)
		assertNull(off.theme)
		assertEquals(OwnTheme(ThemeMode.Dark, pureBlack = true, dynamicColor = false), off.turned(true, Prefs()).theme)
		// Turned off before it was ever on: nothing kept.
		assertNull(Look().turned(false, profile).own)
		// The scale stays as it was.
		assertEquals(120, Look(scale = 120).turned(true, profile).scale)
	}

	@Test
	fun theAppStartsOnTheComputerOpenedLast() {
		val a = Machine("a".repeat(32), "deskpc", listOf("192.0.2.1"), 47823, "f")
		val b = Machine("b".repeat(32), "laptop", listOf("192.0.2.2"), 47823, "f")
		// The one opened last, over Home, whether or not it can be reached now.
		assertEquals(Screen.Machine(b.id), startScreen(b.id, listOf(a, b), link = false))
		// Forgotten, or paired again under a new id: Home.
		assertNull(startScreen("c".repeat(32), listOf(a, b), link = false))
		// Started by a link to pair: the pairing, over Home.
		assertNull(startScreen(b.id, listOf(a, b), link = true))
		// Nothing paired, or none opened yet: Home.
		assertNull(startScreen(b.id, emptyList(), link = false))
		assertNull(startScreen(null, listOf(a, b), link = false))
	}

	@Test
	fun pairedAgainUnderANewIdLeavesTheOldEntryToForget() {
		val old = "0123456789abcdef0123456789abcdef"
		val new = "fedcba9876543210fedcba9876543210"
		assertEquals(old, leftBehind(old, new))
		// The same computer paired again, or a new one: nothing left behind.
		assertNull(leftBehind(old, old))
		assertNull(leftBehind(null, new))
	}
}
