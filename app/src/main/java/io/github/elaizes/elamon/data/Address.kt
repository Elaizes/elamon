package io.github.elaizes.elamon.data

/** Host names and addresses as they come from QR codes, mDNS and what people type. */
object Hosts {
	/** Dotted decimal, four numbers from 0 to 255. */
	fun isIpv4(s: String): Boolean {
		val parts = s.split('.')
		return parts.size == 4 && parts.all { p -> p.length in 1..3 && p.all { it in '0'..'9' } && p.toInt() <= 255 }
	}

	/** An IPv6 literal without brackets or zone: up to eight hex groups, one "::" at most, an IPv4 tail allowed. */
	fun isIpv6(s: String): Boolean {
		val v4 = s.substringAfterLast(':').takeIf { '.' in it }
		if (v4 != null && !isIpv4(v4)) return false
		// An IPv4 tail counts as two groups.
		val halves = (if (v4 != null) s.dropLast(v4.length) + "0:0" else s).split("::")
		if (halves.size > 2) return false
		val groups = halves.flatMap { if (it.isEmpty()) emptyList() else it.split(':') }
		if (!groups.all { it.length in 1..4 && it.all { c -> c.isHexDigit() } }) return false
		return if (halves.size == 2) groups.size <= 7 else groups.size == 8
	}

	/** Letters, digits and hyphens in dot-separated labels of up to 63; the last label isn't all digits. */
	fun isDnsName(s: String): Boolean {
		val name = s.removeSuffix(".")
		if (name.length > DNS_NAME_MAX) return false
		val labels = name.split('.')
		return labels.all { DNS_LABEL.matches(it) } && !labels.last().all { it in '0'..'9' }
	}

	/** Something to connect to: an IPv4 or IPv6 address or a DNS name, nothing else. */
	fun isValid(s: String): Boolean = isIpv4(s) || isIpv6(s) || isDnsName(s)

	/** The longest name the DNS carries; a label is 1 to 63 letters, digits and inner hyphens. */
	private const val DNS_NAME_MAX = 253
	private val DNS_LABEL = Regex("[A-Za-z0-9]([A-Za-z0-9-]{0,61}[A-Za-z0-9])?")

	private fun Char.isHexDigit() = this in '0'..'9' || this in 'a'..'f' || this in 'A'..'F'
}

/** A port as written, or null unless it is a number from 1 to [Limits.PORT_MAX]. */
fun parsePort(s: String): Int? =
	s.takeIf { it.length in 1..Limits.PORT_DIGITS && it.all { c -> c in '0'..'9' } }?.toInt()?.takeIf { it in 1..Limits.PORT_MAX }

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
			rest.startsWith(":") -> parsePort(rest.substring(1)) ?: return null
			else -> return null
		}
		return if (Hosts.isIpv6(host)) host to p else null
	}
	if (s.count { it == ':' } > 1) return if (Hosts.isIpv6(s)) s to defaultPort else null
	val colon = s.indexOf(':')
	val host = if (colon < 0) s else s.substring(0, colon)
	val p = if (colon < 0) defaultPort else parsePort(s.substring(colon + 1)) ?: return null
	return if (Hosts.isIpv4(host) || Hosts.isDnsName(host)) host to p else null
}

/** What a link's address says about the computer itself: the phone can't use these. */
private val LOCAL_HOSTS = setOf("{host}", "localhost", "127.0.0.1", "[::1]")

private val SCHEME = Regex("^[A-Za-z][A-Za-z0-9+.-]*://")

/**
 * A link's address as the phone opens it. "{host}", "localhost" and the loopback address in the
 * host's place become [host], the address this phone reaches the computer at; "{host}" elsewhere
 * becomes it too. A bare host:port gets http://. The rest of the link is left as it is.
 */
fun linkUrl(url: String, host: String): String {
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

private val LINK = Regex("^([A-Za-z][A-Za-z0-9+.-]*)://([^/?#]*)")

/**
 * Whether the app opens [url]: a web page, http or https, with a host, and nothing in it a browser
 * would read otherwise (spaces, control characters). Anything else a service or a link might give
 * (intent:, file:, content:, javascript:, a scheme of another app's) isn't opened.
 */
fun isWebLink(url: String): Boolean {
	if (url.any { it.isWhitespace() || it.isISOControl() || Character.getType(it) == Character.FORMAT.toInt() }) return false
	val m = LINK.find(url) ?: return false
	val scheme = m.groupValues[1].lowercase()
	val host = m.groupValues[2].substringAfterLast('@')
	return (scheme == "http" || scheme == "https") && host.isNotEmpty() && !host.startsWith(":")
}
