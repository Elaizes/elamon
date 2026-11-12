package io.github.elaizes.elamon

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.os.Bundle
import android.widget.Toast
import androidx.activity.compose.BackHandler
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.activity.viewModels
import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.core.tween
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.scaleIn
import androidx.compose.animation.togetherWith
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateListOf
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.toArgb
import androidx.core.content.ContextCompat
import androidx.core.graphics.drawable.toDrawable
import androidx.fragment.app.FragmentActivity
import androidx.lifecycle.ViewModel
import androidx.lifecycle.ViewModelStore
import androidx.lifecycle.ViewModelStoreOwner
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import io.github.elaizes.elamon.data.PairLink
import io.github.elaizes.elamon.data.PairTarget
import io.github.elaizes.elamon.data.Machine as Paired
import io.github.elaizes.elamon.data.MachineStore
import io.github.elaizes.elamon.data.Profile
import io.github.elaizes.elamon.data.UiCache
import io.github.elaizes.elamon.ui.ElamonTheme
import io.github.elaizes.elamon.ui.Guard
import io.github.elaizes.elamon.ui.GuardDialog
import io.github.elaizes.elamon.ui.GuardState
import io.github.elaizes.elamon.ui.HelpScreen
import io.github.elaizes.elamon.ui.HelpTopic
import io.github.elaizes.elamon.ui.HomeScreen
import io.github.elaizes.elamon.ui.LocalGuard
import io.github.elaizes.elamon.ui.MachineScreen
import io.github.elaizes.elamon.ui.Motion
import io.github.elaizes.elamon.ui.PairScreen
import io.github.elaizes.elamon.ui.SettingsScreen
import io.github.elaizes.elamon.ui.appColorScheme
import io.github.elaizes.elamon.ui.forgetUnlock
import io.github.elaizes.elamon.ui.isDark

sealed interface Screen {
	data object Home : Screen
	data class Machine(val id: String) : Screen
	data class Pair(val target: PairTarget) : Screen
	data class Help(val topic: String? = null) : Screen
	/** The settings in force, those of the computer [machine] or of the first one paired. */
	data class Settings(val machine: String? = null) : Screen
}

/** The screen stack; lives through rotation, starts over after the process dies. */
class Nav : ViewModel() {
	val stack = mutableStateListOf<Screen>(Screen.Home)
	val top get() = stack.last()

	/** View models of the screens that keep their own (a computer, a pairing), while they're on the stack. */
	private val stores = mutableMapOf<Screen, ViewModelStore>()

	/** Screens composed now, those animating out included: their view models wait until they're gone. */
	private val shown = mutableMapOf<Screen, Int>()

	fun push(s: Screen) {
		stack.add(s)
	}

	fun pop() {
		if (stack.size > 1) stack.removeAt(stack.lastIndex)
		prune()
	}

	/** Replaces everything above Home, e.g. after pairing. */
	fun reset(s: Screen) {
		while (stack.size > 1) stack.removeAt(stack.lastIndex)
		if (s != Screen.Home) stack.add(s)
		prune()
	}

	/** Goes back to [s] when it's on the stack already, else shows it on top. */
	fun show(s: Screen) {
		val at = stack.lastIndexOf(s)
		if (at < 0) return push(s)
		while (stack.lastIndex > at) stack.removeAt(stack.lastIndex)
		prune()
	}

	/** A computer paired (again): its screen starts afresh, without the old pairing's history or panel. */
	fun paired(id: String) {
		val s = Screen.Machine(id)
		if (s !in shown) stores.remove(s)?.clear()
		reset(s)
	}

	/** Where [s] keeps its view models. */
	fun owner(s: Screen): ViewModelStoreOwner = object : ViewModelStoreOwner {
		override val viewModelStore: ViewModelStore get() = stores.getOrPut(s) { ViewModelStore() }
	}

	fun attach(s: Screen) {
		shown[s] = (shown[s] ?: 0) + 1
	}

	fun detach(s: Screen) {
		val n = (shown[s] ?: 1) - 1
		if (n > 0) shown[s] = n else shown.remove(s)
		prune()
	}

	/** Clears the view models of screens that are gone for good: off the stack and off the screen. */
	private fun prune() {
		for (s in stores.keys.filter { it !in stack && it !in shown }) stores.remove(s)?.clear()
	}

	override fun onCleared() {
		stores.values.forEach { it.clear() }
		stores.clear()
	}
}

/** A FragmentActivity: the unlock prompt lives in one. */
class MainActivity : FragmentActivity() {
	private val nav: Nav by viewModels()
	private val guardState: GuardState by viewModels()

	override fun onCreate(savedInstanceState: Bundle?) {
		super.onCreate(savedInstanceState)
		enableEdgeToEdge()
		// The app's own background from the first frame, not the phone's day or night.
		val cache = UiCache.get(this)
		val store = MachineStore.get(this)
		// The settings of 0.5, once, as a profile to put on the computers.
		Settings.get(this)
		val start = prefsFor(nav.top, store.machines.value, cache.profiles.value)
		window.setBackgroundDrawable(appColorScheme(this, start, isDark(this, start)).surface.toArgb().toDrawable())
		if (savedInstanceState == null) handle(intent)
		val guard = Guard(this, guardState)
		setContent {
			val profiles by cache.profiles.collectAsStateWithLifecycle()
			val paired by store.machines.collectAsStateWithLifecycle()
			val prefs = prefsFor(nav.top, paired, profiles)
			ElamonTheme(prefs) {
				CompositionLocalProvider(LocalGuard provides guard) {
					Surface(Modifier.fillMaxSize(), color = MaterialTheme.colorScheme.surface) {
						BackHandler(enabled = nav.stack.size > 1) { nav.pop() }
						AnimatedContent(
							nav.top,
							transitionSpec = {
								if (prefs.animations) (fadeIn(tween(Motion.SCREEN_IN)) + scaleIn(tween(Motion.SCREEN_IN), initialScale = Motion.SCREEN_SCALE)) togetherWith fadeOut(tween(Motion.SCREEN_OUT))
								else fadeIn(tween(0)) togetherWith fadeOut(tween(0))
							},
							label = "screen",
						) { screen ->
							DisposableEffect(screen) {
								nav.attach(screen)
								onDispose { nav.detach(screen) }
							}
							when (screen) {
								Screen.Home -> HomeScreen(
									onOpen = { nav.push(Screen.Machine(it.id)) },
									onPair = { nav.push(Screen.Pair(it)) },
									onHelp = { nav.push(Screen.Help()) },
									onSettings = { nav.push(Screen.Settings()) },
								)
								is Screen.Machine -> {
									val store = MachineStore.get(this@MainActivity)
									// Watched, not read once: the entry may go or be replaced while the screen is open.
									val machines by store.machines.collectAsStateWithLifecycle()
									val machine = machines.find { it.id == screen.id }
									if (machine == null) {
										LaunchedEffect(screen) {
											// Only the screen on top moves; one on its way out is left alone.
											if (nav.top != screen) return@LaunchedEffect
											// Paired again under a new id, with the pairing screen left before it
											// finished: on to the new entry. Otherwise it's gone: back home.
											val next = store.successor(screen.id)
											if (next != null) nav.paired(next) else nav.reset(Screen.Home)
										}
									} else {
										MachineScreen(
											machine,
											owner = nav.owner(screen),
											onBack = { nav.pop() },
											onPairAgain = { nav.push(Screen.Pair(it)) },
											onForgotten = { nav.reset(Screen.Home) },
											onHelp = { nav.push(Screen.Help(it)) },
											onSettings = { nav.push(Screen.Settings(screen.id)) },
										)
									}
								}
								is Screen.Pair -> PairScreen(
									screen.target,
									owner = nav.owner(screen),
									onBack = { nav.pop() },
									onPaired = { nav.paired(it.id) },
									onHelp = { nav.push(Screen.Help(HelpTopic.TROUBLESHOOTING)) },
								)
								is Screen.Help -> HelpScreen(screen.topic, onBack = { nav.pop() })
								is Screen.Settings -> SettingsScreen(screen.machine, onBack = { nav.pop() }, onHelp = { nav.push(Screen.Help()) })
							}
						}
					}
					// One confirmation for every action that may ask, over whatever screen asked.
					GuardDialog(guard)
				}
			}
		}
	}

	override fun onNewIntent(intent: Intent) {
		super.onNewIntent(intent)
		// Whatever handed it over may have been under a finger: taps that land on pairing wait a moment.
		Arrival.now()
		handle(intent)
	}

	/** The screen going off ends any grace an unlock left, even while the app stays in front. */
	private val screenOff = object : BroadcastReceiver() {
		override fun onReceive(context: Context, intent: Intent) {
			if (intent.action == Intent.ACTION_SCREEN_OFF) forgetUnlock()
		}
	}

	override fun onStart() {
		super.onStart()
		ContextCompat.registerReceiver(this, screenOff, IntentFilter(Intent.ACTION_SCREEN_OFF), ContextCompat.RECEIVER_NOT_EXPORTED)
	}

	override fun onResume() {
		super.onResume()
		// Brought to the front, maybe by another app over what was being tapped there.
		Arrival.now()
	}

	override fun onStop() {
		super.onStop()
		unregisterReceiver(screenOff)
		// To the background (or the screen off): the next action asks again. A rotation isn't leaving.
		if (!isChangingConfigurations) forgetUnlock()
	}

	/**
	 * elamon://pair?… from the QR code that `elamond pair` prints. Any web page can open one too,
	 * so it only ever shows the pairing screen, once, and waits for the user there.
	 */
	private fun handle(intent: Intent?) {
		val uri = intent?.data ?: return
		when (val link = PairTarget.fromUri(uri)) {
			null -> {}
			is PairLink.Target -> nav.show(Screen.Pair(link.target))
			PairLink.Newer -> Toast.makeText(this, R.string.pair_link_newer, Toast.LENGTH_LONG).show()
			PairLink.Invalid -> Toast.makeText(this, R.string.pair_link_invalid, Toast.LENGTH_LONG).show()
		}
	}
}

/**
 * How the app looks on [top]: as the profile of the computer it shows says (a computer's screen,
 * or the settings opened from it), elsewhere as the first paired computer's, as last heard; with
 * none, as the built-in `default` profile.
 */
fun prefsFor(top: Screen, paired: List<Paired>, profiles: Map<String, Profile>): Prefs {
	val id = when (top) {
		is Screen.Machine -> top.id
		is Screen.Settings -> top.machine ?: paired.firstOrNull()?.id
		else -> paired.firstOrNull()?.id
	}
	return id?.let { profiles[it] }?.let(Prefs::of) ?: Prefs()
}
