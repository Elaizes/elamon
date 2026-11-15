package io.github.elaizes.elamon.data

import io.github.elaizes.elamon.BuildConfig
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse

class BodiesTest {
	@Test
	fun everyRequestSaysOnlyThatItIsElamonAndTakesNothingCompressed() {
		val h = requestHeaders(null).toMap()
		assertEquals("Elamon/${BuildConfig.VERSION_NAME}", h["User-Agent"])
		// Every request, with a token or without, says what kind of client it is (API 2).
		assertEquals("android/${BuildConfig.VERSION_NAME}", h["Elamon-Client"])
		assertEquals("android/${BuildConfig.VERSION_NAME}", requestHeaders("t0ken").toMap()["Elamon-Client"])
		assertEquals("identity", h["Accept-Encoding"])
		assertEquals("application/json", h["Accept"])
		assertFalse("Authorization" in h)
		assertEquals("Bearer t0ken", requestHeaders("t0ken").toMap()["Authorization"])
		// Nothing of the phone: Android's own User-Agent names its model and build.
		for (word in listOf("Android", "Dalvik", "Linux", "Build")) assertFalse(word in USER_AGENT, word)
	}
}
