package io.github.elaizes.elamon

import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertNull
import kotlinx.serialization.json.JsonObject
import io.github.elaizes.elamon.data.Action

/** What the app says once an action is done, by its kind. */
class ActionDoneTest {
	private val none = JsonObject(emptyMap())

	@Test
	fun theSettingsAndAStationsSayDone() {
		assertEquals(Message.Res(R.string.done_ran, listOf("Mode")), done(Action("mode", "Mode", kind = "actions"), none))
		// A station's command or setting: it answers once the station did it, which may take a while.
		assertEquals(Message.Res(R.string.done_ran, listOf("Charge limit (percent)")), done(Action("nut.set.eco.experimental.battery.charge.max", "Charge limit (percent)", kind = "nut"), none))
		// The volume shows by itself.
		assertNull(done(Action("volume.set", "Volume", kind = "media"), none))
		assertEquals(Message.Res(R.string.done_restarting), done(Action("power.reboot", "Restart", kind = "power"), none))
	}
}
