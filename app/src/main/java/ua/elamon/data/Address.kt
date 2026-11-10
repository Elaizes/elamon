package ua.elamon.data

/** Host names and addresses as they come from QR codes, mDNS and what people type. */
object Hosts {
	/** Dotted decimal, four numbers from 0 to 255. */
	fun isIpv4(s: String): Boolean {
		val parts = s.split('.')
		return parts.size == 4 && parts.all { p -> p.length in 1..3 && p.all { it in '0'..'9' } && p.toInt() <= 255 }
	}

	/** An IPv6 literal without brackets or zone: hex groups, one "::" at most, an IPv4 tail allowed. */
	fun isIpv6(s: String): Boolean {
		if (s.length < 2 || s.count { it == ':' } < 2) return false
		val double = s.indexOf("::")
		if (double >= 0 && s.indexOf("::", double + 1) >= 0) return false
		val groups = mutableListOf<String>()
		var tailV4 = false
		val head = if (double >= 0) s.substring(0, double) else s
		val tail = if (double >= 0) s.substring(double + 2) else ""
		for ((i, part) in listOf(head, tail).withIndex()) {
			if (part.isEmpty()) continue
			val gs = part.split(':')
			for ((j, g) in gs.withIndex()) {
				// The last group of the whole address may be an IPv4 address, counting as two.
				val last = j == gs.lastIndex && (i == 1 || double < 0)
				if (last && g.contains('.')) {
					if (!isIpv4(g)) return false
					tailV4 = true
				} else {
					if (g.length !in 1..4 || !g.all { it.isHexDigitAscii() }) return false
					groups += g
				}
			}
		}
		val count = groups.size + if (tailV4) 2 else 0
		return if (double >= 0) count <= 7 else count == 8
	}

	/** Letters, digits and hyphens in dot-separated labels; the last label isn't all digits. */
	fun isDnsName(s: String): Boolean {
		val name = s.removeSuffix(".")
		if (name.isEmpty() || name.length > 253) return false
		val labels = name.split('.')
		if (labels.any { l -> l.length !in 1..63 || l.startsWith('-') || l.endsWith('-') || !l.all { it.isLetterOrDigitAscii() || it == '-' } }) return false
		return !labels.last().all { it in '0'..'9' }
	}

	/** Something to connect to: an IPv4 or IPv6 address or a DNS name, nothing else. */
	fun isValid(s: String): Boolean = isIpv4(s) || isIpv6(s) || isDnsName(s)

	private fun Char.isHexDigitAscii() = this in '0'..'9' || this in 'a'..'f' || this in 'A'..'F'
	private fun Char.isLetterOrDigitAscii() = this in '0'..'9' || this in 'a'..'z' || this in 'A'..'Z'
}

/** A port as written, or null unless it is a number from 1 to 65535. */
private fun port(s: String): Int? = s.takeIf { it.length in 1..5 && it.all { c -> c in '0'..'9' } }?.toInt()?.takeIf { it in 1..65535 }

/**
 * An address as typed: "host", "host:port", "[v6]", "[v6]:port" or a bare IPv6 address, with or
 * without http:// or https:// and a trailing slash. Null for anything else, a bad port included.
 */
fun parseAddress(input: String, defaultPort: Int): Pair<String, Int>? {
	var s = input.trim()
	for (scheme in listOf("https://", "http://")) {
		if (s.startsWith(scheme, ignoreCase = true)) s = s.substring(scheme.length)
	}
	s = s.removeSuffix("/")
	if (s.isEmpty()) return null
	if (s.startsWith("[")) {
		val end = s.indexOf(']')
		if (end < 0) return null
		val host = s.substring(1, end)
		val rest = s.substring(end + 1)
		val p = when {
			rest.isEmpty() -> defaultPort
			rest.startsWith(":") -> port(rest.substring(1)) ?: return null
			else -> return null
		}
		return if (Hosts.isIpv6(host)) host to p else null
	}
	if (s.count { it == ':' } > 1) return if (Hosts.isIpv6(s)) s to defaultPort else null
	val colon = s.indexOf(':')
	val host = if (colon < 0) s else s.substring(0, colon)
	val p = if (colon < 0) defaultPort else port(s.substring(colon + 1)) ?: return null
	return if (Hosts.isIpv4(host) || Hosts.isDnsName(host)) host to p else null
}

/** What a service's address says about the computer itself: the phone can't use these. */
private val LOCAL_HOSTS = setOf("{host}", "localhost", "127.0.0.1", "[::1]")

private val SCHEME = Regex("^[A-Za-z][A-Za-z0-9+.-]*://")

/**
 * A service's link as the phone opens it. "{host}", "localhost" and the loopback address in the
 * host's place become [host], the address this phone reaches the computer at; "{host}" elsewhere
 * becomes it too. A bare host:port gets http://. The rest of the link is left as it is.
 */
fun serviceUrl(url: String, host: String): String {
	val h = if (host.contains(':')) "[$host]" else host
	val u = url.trim().let { if (SCHEME.containsMatchIn(it)) it else "http://$it" }
	val start = u.indexOf("://") + 3
	val end = u.indexOfAny(charArrayOf('/', '?', '#'), start).let { if (it < 0) u.length else it }
	val authority = u.substring(start, end)
	val at = authority.lastIndexOf('@')
	val userInfo = authority.substring(0, at + 1)
	val hostPort = authority.substring(at + 1)
	val split = if (hostPort.startsWith("[")) {
		hostPort.indexOf(']').let { if (it < 0) hostPort.length else it + 1 }
	} else {
		hostPort.lastIndexOf(':').let { if (it < 0) hostPort.length else it }
	}
	val name = hostPort.substring(0, split)
	val newName = if (name.lowercase() in LOCAL_HOSTS) h else name
	val rest = u.substring(end).replace("{host}", host)
	return u.substring(0, start) + userInfo + newName + hostPort.substring(split) + rest
}
