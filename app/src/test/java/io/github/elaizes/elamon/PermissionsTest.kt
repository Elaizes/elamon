package io.github.elaizes.elamon

import java.io.File
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertNull
import kotlin.test.assertTrue
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import io.github.elaizes.elamon.data.ControlInfo
import io.github.elaizes.elamon.data.Display
import io.github.elaizes.elamon.data.LinkStatus
import io.github.elaizes.elamon.data.Media
import io.github.elaizes.elamon.data.Proc
import io.github.elaizes.elamon.data.ProtocolJson
import io.github.elaizes.elamon.data.Sample

class PermissionsTest {
	/** What elamond lists for [role] with control [on]: its own rule, every name the role reaches, control-bound ones only with control. */
	private fun served(role: PhoneRole, on: Boolean): List<String> =
		Can.entries.filter { role >= it.role && (on || !it.needsControl) }.map { it.key }

	private fun control(role: String?, allowed: List<String>?, enabled: Boolean = true) =
		ControlInfo(enabled = enabled, role = role, allowed = allowed)

	private fun keys(p: Permissions) = p.allowed.map { it.key }.toSet()

	@Test
	fun eachRoleWithControlOnAndOff() {
		for (role in PhoneRole.entries) for (on in listOf(true, false)) {
			val p = Permissions.of(control(role.key, served(role, on), enabled = on))
			assertEquals(served(role, on).toSet(), keys(p), "$role, control $on")
			assertEquals(role, p.known)
		}
	}

	@Test
	fun glanceSeesTheBasicsOnly() {
		val p = Permissions.of(control("glance", emptyList()))
		assertTrue(p.allowed.isEmpty())
		assertFalse(p.processes || p.displays || p.links || p.mediaRead || p.log)
		assertFalse(p.mayAct)
		assertFalse(p.roleAllows(Can.Units))
	}

	@Test
	fun monitorSeesEverythingAndChangesNothing() {
		val p = Permissions.of(control("monitor", served(PhoneRole.Monitor, true)))
		assertTrue(p.processes && p.displays && p.links && p.mediaRead && p.log)
		assertFalse(p.linksEdit || p.units || p.media || p.volume || p.profile || p.actions || p.power || p.signal)
		// Control being off is nothing to a monitor: it couldn't act with it on.
		assertFalse(p.mayAct)
	}

	@Test
	fun operateActsButNeitherPowersNorSignals() {
		val on = Permissions.of(control("operate", served(PhoneRole.Operate, true)))
		assertTrue(on.linksEdit && on.units && on.media && on.volume && on.profile && on.actions)
		assertFalse(on.power || on.signal)
		val off = Permissions.of(control("operate", served(PhoneRole.Operate, false), enabled = false))
		// Links are only the app's list: changed without control.
		assertTrue(off.linksEdit)
		assertFalse(off.units || off.media || off.volume || off.profile || off.actions)
		assertTrue(off.mayAct)
		assertTrue(off.roleAllows(Can.Units))
		assertFalse(off.roleAllows(Can.Power))
	}

	@Test
	fun adminWithControlOffActsOnNothing() {
		val p = Permissions.of(control("admin", served(PhoneRole.Admin, false), enabled = false))
		assertEquals(setOf("processes", "displays", "links", "media.read", "log", "links.edit"), keys(p))
		assertTrue(p.mayAct && p.roleAllows(Can.Signal))
		assertFalse(p.signal)
	}

	@Test
	fun theServiceDecidesNotTheRole() {
		// Whatever the role says, only what's listed is allowed, in case the service knows better.
		val p = Permissions.of(control("admin", listOf("processes", "log")))
		assertEquals(setOf("processes", "log"), keys(p))
		assertTrue(p.roleAllows(Can.Power))
	}

	@Test
	fun namesFromNewerServicesAreLeftAlone() {
		val p = Permissions.of(control("admin", listOf("processes", "teleport", "signal")))
		assertEquals(setOf("processes", "signal"), keys(p))
		// A role this app doesn't know allows only what it's told.
		val owner = Permissions.of(control("owner", listOf("power")))
		assertNull(owner.known)
		assertTrue(owner.power)
		assertFalse(owner.roleAllows(Can.Processes))
		assertFalse(owner.mayAct)
	}

	@Test
	fun aServiceThatDoesntSayShowsWhatTheSamplesCarry() {
		// No role and nothing allowed by name: a service older than this app works with.
		for (on in listOf(true, false)) {
			val p = Permissions.of(control(null, null, enabled = on))
			assertNull(p.role)
			assertEquals(setOf("processes", "displays", "links"), keys(p))
			assertFalse(p.mayAct || p.roleAllows(Can.Signal) || p.roleAllows(Can.Processes))
		}
	}

	@Test
	fun beforeThePanelIsRead() {
		// What the last known role lets this phone see, nothing that needs control.
		assertTrue(Permissions.before("glance").allowed.isEmpty())
		assertEquals(setOf("processes", "displays", "links", "media.read", "log"), keys(Permissions.before("monitor")))
		assertEquals(setOf("processes", "displays", "links", "media.read", "log", "links.edit"), keys(Permissions.before("operate")))
		assertEquals(keys(Permissions.before("operate")), keys(Permissions.before("admin")))
		// No role known yet: what the samples carry, as from a service that doesn't say.
		assertEquals(keys(Permissions.of(control(null, null, enabled = false))), keys(Permissions.before(null)))
		assertTrue(Permissions.before("owner").allowed.isEmpty())
		assertEquals("monitor", Permissions.before("monitor").role)
	}

	@Test
	fun aSampleWithMoreThanThePhoneSeesMeansItsRoleWentUp() {
		val glance = Permissions.of(control("glance", emptyList()))
		val monitor = Permissions.of(control("monitor", served(PhoneRole.Monitor, true)))
		val basics = Sample(t = 1, uptime = 1)
		assertFalse(glance.outgrownBy(basics))
		val more = listOf(
			basics.copy(processes = listOf(Proc(pid = 1, name = "init"))),
			basics.copy(displays = listOf(Display("DP-1"))),
			basics.copy(links = listOf(LinkStatus("a"))),
			basics.copy(media = Media()),
		)
		for (s in more) {
			assertTrue(glance.outgrownBy(s), s.toString())
			assertFalse(monitor.outgrownBy(s), s.toString())
		}
	}

	@Test
	fun theAppKnowsEveryNameTheServiceHasInItsOrder() {
		// elamond/fixtures/control.json is an admin with control on: everything there is.
		val dir = File(System.getProperty("elamon.fixtures") ?: error("elamon.fixtures isn't set; run the tests through Gradle"))
		val raw = Json.parseToJsonElement(File(dir, "control.json").readText()).jsonObject
		val names = raw.getValue("allowed").jsonArray.map { it.jsonPrimitive.content }
		assertEquals(names, Can.entries.map { it.key })
		val p = Permissions.of(ProtocolJson.decodeFromJsonElement(ControlInfo.serializer(), raw))
		assertEquals(Can.entries.toSet(), p.allowed)
		assertEquals(PhoneRole.Admin, p.known)
		assertEquals(PhoneRole.entries.map { it.key }, listOf("glance", "monitor", "operate", "admin"))
	}
}
