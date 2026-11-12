package io.github.elaizes.elamon

/**
 * A paired phone's role on a computer, lowest first: each allows what the one before does, and
 * more. The service filters what it describes and sends by it; the app only names it.
 */
enum class PhoneRole(val key: String) {
	Glance("glance"),
	Monitor("monitor"),
	Operate("operate"),
	Admin("admin");

	companion object {
		/** The role the service names; null for none, or one from a newer service this app doesn't know. */
		fun of(key: String?): PhoneRole? = entries.find { it.key == key }
	}
}
