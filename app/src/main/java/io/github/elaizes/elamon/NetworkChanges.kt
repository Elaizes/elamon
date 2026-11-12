package io.github.elaizes.elamon

import android.content.Context
import android.net.ConnectivityManager
import android.net.Network
import android.net.NetworkCapabilities
import java.util.concurrent.atomic.AtomicBoolean
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update

/**
 * Counts the times the phone's network changed since the app started: another network in use
 * (Wi-Fi, mobile data, a VPN coming up or going down), what the one in use goes over (Wi-Fi or
 * mobile data under a VPN), or none left. A computer looked for in vain may answer now at another of
 * its addresses, so a change is reason to look again at once.
 */
object NetworkChanges {
	private val _count = MutableStateFlow(0L)
	val count: StateFlow<Long> = _count.asStateFlow()

	private val watching = AtomicBoolean(false)

	/** Starts watching, once a run. A phone that won't tell ([ConnectivityManager] refusing) never counts a change. */
	fun watch(context: Context) {
		if (!watching.compareAndSet(false, true)) return
		val cm = context.applicationContext.getSystemService(ConnectivityManager::class.java) ?: return
		runCatching { cm.registerDefaultNetworkCallback(Watch()) }
	}

	/** The network in use, and what it goes over: a change of either is one. */
	private data class Seen(val network: Network?, val transports: Set<Int>)

	private class Watch : ConnectivityManager.NetworkCallback() {
		private var current: Seen? = null

		// Told after every network that comes into use, and when what it goes over changes.
		override fun onCapabilitiesChanged(network: Network, caps: NetworkCapabilities) {
			seen(Seen(network, TRANSPORTS.filterTo(mutableSetOf()) { caps.hasTransport(it) }))
		}

		override fun onLost(network: Network) {
			if (current?.network == network) seen(Seen(null, emptySet()))
		}

		@Synchronized
		private fun seen(now: Seen) {
			val before = current
			current = now
			// The first is where the phone is as watching starts, not a change.
			if (before != null && before != now) _count.update { it + 1 }
		}
	}

	/** What a network may go over, as the phone tells it; signal strength and the like change too often to count. */
	private val TRANSPORTS = listOf(
		NetworkCapabilities.TRANSPORT_WIFI,
		NetworkCapabilities.TRANSPORT_CELLULAR,
		NetworkCapabilities.TRANSPORT_ETHERNET,
		NetworkCapabilities.TRANSPORT_VPN,
		NetworkCapabilities.TRANSPORT_BLUETOOTH,
	)
}
