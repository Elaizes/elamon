package io.github.elaizes.elamon.data

import io.github.elaizes.elamon.BuildConfig

/**
 * What the app says it is to the service, and nothing more: Android's own User-Agent names the
 * phone's model and build.
 */
val USER_AGENT = "Elamon/${BuildConfig.VERSION_NAME}"

/**
 * What kind of client this is, for the service (protocol API 2): it shows it in `elamond clients`
 * and gives the app the profile's part for Android.
 */
val CLIENT = "android/${BuildConfig.VERSION_NAME}"

/**
 * The headers every request carries. "identity": answers come as they are, never compressed.
 */
internal fun requestHeaders(token: String?): List<Pair<String, String>> = listOfNotNull(
	"Accept" to "application/json",
	"Accept-Encoding" to "identity",
	"User-Agent" to USER_AGENT,
	"Elamon-Client" to CLIENT,
	token?.let { "Authorization" to "Bearer $it" },
)
