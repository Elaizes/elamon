package io.github.elaizes.elamon

import io.github.elaizes.elamon.data.ControlInfo
import io.github.elaizes.elamon.data.Sample

/** A paired phone's role on a computer, lowest first: each allows what the one before does, and more. */
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

/**
 * One thing a phone may be allowed, by its name in `allowed` from GET /api/v1/control: the lowest
 * role that may, and whether it also needs control on. In the service's order.
 */
enum class Can(val key: String, val role: PhoneRole, val needsControl: Boolean) {
	Processes("processes", PhoneRole.Monitor, false),
	Displays("displays", PhoneRole.Monitor, false),
	Links("links", PhoneRole.Monitor, false),
	MediaRead("media.read", PhoneRole.Monitor, false),
	Log("log", PhoneRole.Monitor, false),
	LinksEdit("links.edit", PhoneRole.Operate, false),
	Units("units", PhoneRole.Operate, true),
	Media("media", PhoneRole.Operate, true),
	Volume("volume", PhoneRole.Operate, true),
	Profile("profile", PhoneRole.Operate, true),
	Actions("actions", PhoneRole.Operate, true),
	Power("power", PhoneRole.Admin, true),
	Signal("signal", PhoneRole.Admin, true);

	companion object {
		/** Null for a name from a newer service: it is left alone. */
		fun of(key: String): Can? = entries.find { it.key == key }
	}
}

/**
 * What this phone may do on a computer now. The screens leave out whatever isn't in [allowed]
 * rather than offer it and be refused.
 */
data class Permissions(
	/** The role as the service names it; null from elamond before 0.5, which has no roles. */
	val role: String?,
	val allowed: Set<Can>,
) {
	val processes: Boolean get() = Can.Processes in allowed
	val displays: Boolean get() = Can.Displays in allowed
	val links: Boolean get() = Can.Links in allowed
	val mediaRead: Boolean get() = Can.MediaRead in allowed
	val log: Boolean get() = Can.Log in allowed
	val linksEdit: Boolean get() = Can.LinksEdit in allowed
	val units: Boolean get() = Can.Units in allowed
	val media: Boolean get() = Can.Media in allowed
	val volume: Boolean get() = Can.Volume in allowed
	val profile: Boolean get() = Can.Profile in allowed
	val actions: Boolean get() = Can.Actions in allowed
	val power: Boolean get() = Can.Power in allowed
	val signal: Boolean get() = Can.Signal in allowed

	/** The role, when this app knows it. */
	val known: PhoneRole? get() = PhoneRole.of(role)

	/**
	 * Whether the role allows [can], control or not: when it does but [can] isn't allowed, control
	 * is off. Without roles (elamond before 0.5) every phone may everything; a role this app doesn't
	 * know, nothing it isn't told.
	 */
	fun roleAllows(can: Can): Boolean = role == null || known?.let { it >= can.role } == true

	/** Whether the role lets this phone act on the computer at all: only then is control being off worth telling. */
	val mayAct: Boolean get() = Can.entries.any { it.needsControl && roleAllows(it) }

	/**
	 * Whether [sample] holds something this phone isn't known to see: then its role went up on the
	 * computer, and the control panel is worth reading again.
	 */
	fun outgrownBy(sample: Sample): Boolean =
		(sample.processes != null && !processes) ||
			(sample.displays.isNotEmpty() && !displays) ||
			(sample.links.isNotEmpty() && !links) ||
			(sample.media != null && !mediaRead)

	companion object {
		/** What needs no control: everything a role allows while control is off. */
		private val WITHOUT_CONTROL = Can.entries.filter { !it.needsControl }.toSet()

		/** What [control] says this phone may do. */
		fun of(control: ControlInfo): Permissions {
			val allowed = control.allowed ?: return old(control.enabled)
			return Permissions(control.role, allowed.mapNotNull(Can::of).toSet())
		}

		/**
		 * elamond before 0.5 says nothing of roles: a phone could do everything there, acting only
		 * with control on, which also covered media and their covers. It keeps no activity log.
		 */
		private fun old(enabled: Boolean): Permissions {
			val allowed = if (enabled) Can.entries.toSet() else WITHOUT_CONTROL - Can.MediaRead
			return Permissions(null, allowed - Can.Log)
		}

		/**
		 * Before the control panel is read: what a phone with [role], as last heard, may see. Acting
		 * waits for the panel, which says whether control is on; a role this app doesn't know shows
		 * the basics only until then. Without a role, as an older pairing or service, it is what an
		 * older service lets every phone see.
		 */
		fun before(role: String?): Permissions {
			if (role == null) return old(enabled = false)
			val known = PhoneRole.of(role) ?: return Permissions(role, emptySet())
			return Permissions(role, WITHOUT_CONTROL.filter { known >= it.role }.toSet())
		}
	}
}
