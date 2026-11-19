package io.github.elaizes.elamon

import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertNull
import kotlin.test.assertTrue

class PhoneRoleTest {
	@Test
	fun rolesByTheNamesTheServiceGives() {
		assertEquals(PhoneRole.Glance, PhoneRole.of("glance"))
		assertEquals(PhoneRole.Admin, PhoneRole.of("admin"))
		// elamond 0.11's role above admin.
		assertEquals(PhoneRole.Owner, PhoneRole.of("owner"))
		// None, and one of a newer service still: shown by its name.
		assertNull(PhoneRole.of(null))
		assertNull(PhoneRole.of("keeper"))
		assertNull(PhoneRole.of("Owner"))
	}

	@Test
	fun lowestFirst() {
		assertEquals(listOf("glance", "monitor", "operate", "admin", "owner"), PhoneRole.entries.map { it.key })
		assertTrue(PhoneRole.Owner > PhoneRole.Admin)
	}
}
