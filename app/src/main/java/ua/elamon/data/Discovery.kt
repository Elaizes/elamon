package ua.elamon.data

import android.content.Context
import android.net.nsd.NsdManager
import android.net.nsd.NsdServiceInfo
import android.os.Build
import kotlinx.coroutines.channels.awaitClose
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.callbackFlow
import java.net.Inet4Address
import java.util.concurrent.Executors

/** An service announcing itself on the local network. */
data class Found(
	val id: String,
	val name: String,
	val host: String,
	val port: Int,
	val fingerprintPrefix: String?,
)

private const val SERVICE_TYPE = "_elamon._tcp."

/** Services on the local network, updated as they come and go. Empty where multicast doesn't reach (VPNs). */
fun discoverServices(context: Context): Flow<List<Found>> = callbackFlow {
	val nsd = context.getSystemService(Context.NSD_SERVICE) as NsdManager
	val found = linkedMapOf<String, Found>()
	val executor = Executors.newSingleThreadExecutor()
	// Before Android 14 a resolver handles one service at a time; queue the rest.
	val queue = ArrayDeque<NsdServiceInfo>()
	var resolving = false

	fun publish() {
		trySend(found.values.sortedBy { it.name.lowercase() })
	}

	fun resolveNext() {
		if (resolving) return
		val next = queue.removeFirstOrNull() ?: return
		resolving = true
		@Suppress("DEPRECATION")
		nsd.resolveService(next, object : NsdManager.ResolveListener {
			override fun onResolveFailed(info: NsdServiceInfo, errorCode: Int) {
				executor.execute {
					resolving = false
					resolveNext()
				}
			}

			override fun onServiceResolved(info: NsdServiceInfo) {
				executor.execute {
					resolving = false
					val attrs = info.attributes
					fun attr(k: String) = attrs[k]?.let { String(it, Charsets.UTF_8) }
					val host = hostOf(info)
					val id = attr("id")
					if (host != null && id != null) {
						found[info.serviceName] = Found(id, info.serviceName, host, info.port, attr("fp"))
						publish()
					}
					resolveNext()
				}
			}
		})
	}

	val listener = object : NsdManager.DiscoveryListener {
		override fun onDiscoveryStarted(serviceType: String) {}
		override fun onDiscoveryStopped(serviceType: String) {}
		override fun onStartDiscoveryFailed(serviceType: String, errorCode: Int) {}
		override fun onStopDiscoveryFailed(serviceType: String, errorCode: Int) {}

		override fun onServiceFound(info: NsdServiceInfo) {
			executor.execute {
				queue.addLast(info)
				resolveNext()
			}
		}

		override fun onServiceLost(info: NsdServiceInfo) {
			executor.execute {
				if (found.remove(info.serviceName) != null) publish()
			}
		}
	}

	trySend(emptyList())
	runCatching { nsd.discoverServices(SERVICE_TYPE, NsdManager.PROTOCOL_DNS_SD, listener) }
	awaitClose {
		runCatching { nsd.stopServiceDiscovery(listener) }
		executor.shutdown()
	}
}

private fun hostOf(info: NsdServiceInfo): String? {
	val addrs = if (Build.VERSION.SDK_INT >= 34) {
		info.hostAddresses
	} else {
		@Suppress("DEPRECATION")
		listOfNotNull(info.host)
	}
	// IPv4 first: link-local IPv6 needs a scope the URL can't carry.
	val best = addrs.firstOrNull { it is Inet4Address } ?: addrs.firstOrNull { !it.isLinkLocalAddress }
	return best?.hostAddress
}
