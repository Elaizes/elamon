package io.github.elaizes.elamon

import android.content.Context
import android.content.Intent
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
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.toArgb
import androidx.core.graphics.drawable.toDrawable
import androidx.fragment.app.FragmentActivity
import androidx.glance.appwidget.updateAll
import androidx.lifecycle.ViewModel
import androidx.lifecycle.ViewModelStore
import androidx.lifecycle.ViewModelStoreOwner
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import kotlinx.coroutines.launch
import io.github.elaizes.elamon.data.PairLink
import io.github.elaizes.elamon.data.PairTarget
import io.github.elaizes.elamon.data.Machine as Paired
import io.github.elaizes.elamon.data.MachineStore
import io.github.elaizes.elamon.data.Profile
import io.github.elaizes.elamon.data.UiCache
import io.github.elaizes.elamon.ui.EditorScreen
import io.github.elaizes.elamon.ui.ElamonTheme
import io.github.elaizes.elamon.ui.ForgottenUntold
import io.github.elaizes.elamon.ui.Guard
import io.github.elaizes.elamon.ui.GuardDialog
import io.github.elaizes.elamon.ui.GuardState
import io.github.elaizes.elamon.ui.HelpScreen
import io.github.elaizes.elamon.ui.HomeScreen
import io.github.elaizes.elamon.ui.LocalGuard
import io.github.elaizes.elamon.ui.MachineScreen
import io.github.elaizes.elamon.ui.Motion
import io.github.elaizes.elamon.ui.PairScreen
import io.github.elaizes.elamon.ui.SettingsScreen
import io.github.elaizes.elamon.ui.appColorScheme
import io.github.elaizes.elamon.ui.forgetUnlock
import io.github.elaizes.elamon.ui.isDark
import io.github.elaizes.elamon.ui.scaledConfiguration
import io.github.elaizes.elamon.widget.ElamonWidget

sealed interface Screen {
	data object Home : Screen
	data class Machine(val id: String) : Screen
	data class Pair(val target: PairTarget) : Screen
	data object Help : Screen
	/** The settings in force, those of the computer [machine] or of the first one paired. */
	data class Settings(val machine: String? = null) : Screen

	/**
	 * The dashboard editor of the computer [machine] (elamond 0.11): on [dashboard] as its screen
	 * names it, a new one without it, or with [tabs] the tabs of its profile.
	 */
	data class Editor(val machine: String, val dashboard: String? = null, val tabs: Boolean = false) : Screen
}

/** The screen stack; lives through rotation, starts over after the process dies. */
class Nav : ViewModel() {
	val stack = mutableStateListOf<Screen>(Screen.Home)
	val top get() = stack.last()

	/** A computer just forgotten that couldn't be told to unpair the phone, by its name: said over home. */
	var untold by mutableStateOf<String?>(null)

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

	/** The app's scale this activity was made at; another makes it again. */
	private var madeAt = Scale.DEFAULT

	override fun attachBaseContext(base: Context) {
		super.attachBaseContext(base)
		madeAt = Settings.get(base).look.value.scale
		scaledConfiguration(base.resources.configuration, madeAt)?.let(::applyOverrideConfiguration)
	}

	override fun onCreate(savedInstanceState: Bundle?) {
		super.onCreate(savedInstanceState)
		enableEdgeToEdge()
		// The app's own background from the first frame, not the phone's day or night.
		val cache = UiCache.get(this)
		val store = MachineStore.get(this)
		val settings = Settings.get(this)
		if (savedInstanceState == null) {
			// A link the task was started by, brought back from recents, was taken up when it came.
			val link = intent?.takeIf { it.flags and Intent.FLAG_ACTIVITY_LAUNCHED_FROM_HISTORY == 0 }
			// Started afresh: on the computer a widget opened, or the one opened last, Home under it; a
			// link to pair comes first.
			val asked = link?.getStringExtra(EXTRA_MACHINE)?.takeIf { id -> store.machines.value.any { it.id == id } }
			startScreen(asked ?: settings.lastMachine(), store.machines.value, link = asked == null && link?.data != null)?.let(nav::push)
			handle(link)
		}
		val start = prefsFor(nav.top, store.machines.value, cache.profiles.value, settings.look.value.theme)
		window.setBackgroundDrawable(appColorScheme(this, start, isDark(this, start)).surface.toArgb().toDrawable())
		val guard = Guard(this, guardState)
		setContent {
			val profiles by cache.profiles.collectAsStateWithLifecycle()
			val paired by store.machines.collectAsStateWithLifecycle()
			val look by settings.look.collectAsStateWithLifecycle()
			// Another scale: made again at it, as after a turn of the phone.
			LaunchedEffect(look.scale) { if (look.scale != madeAt) recreate() }
			// The home-screen widgets at the app's look and scale too: their own drawing may have ended.
			LaunchedEffect(look) { Outside.scope.launch { runCatching { ElamonWidget().updateAll(applicationContext) } } }
			val prefs = prefsFor(nav.top, paired, profiles, look.theme)
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
									onHelp = { nav.push(Screen.Help) },
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
										// The computer the app starts on next time.
										LaunchedEffect(screen) { settings.setLastMachine(screen.id) }
										MachineScreen(
											machine,
											owner = nav.owner(screen),
											onBack = { nav.pop() },
											onPairAgain = { nav.push(Screen.Pair(it)) },
											onForgotten = { untold ->
												nav.untold = untold
												nav.reset(Screen.Home)
											},
											onHelp = { nav.push(Screen.Help) },
											onSettings = { nav.push(Screen.Settings(screen.id)) },
											onEdit = { d, tabs -> nav.push(Screen.Editor(screen.id, d, tabs)) },
										)
									}
								}
								is Screen.Pair -> PairScreen(
									screen.target,
									owner = nav.owner(screen),
									onBack = { nav.pop() },
									onPaired = { nav.paired(it.id) },
									onHelp = { nav.push(Screen.Help) },
								)
								is Screen.Editor -> {
									val machines by MachineStore.get(this@MainActivity).machines.collectAsStateWithLifecycle()
									val machine = machines.find { it.id == screen.machine }
									if (machine == null) {
										// Forgotten meanwhile: its screen goes, and the editor with it.
										LaunchedEffect(screen) { if (nav.top == screen) nav.reset(Screen.Home) }
									} else {
										EditorScreen(
											machine,
											screen.dashboard,
											screen.tabs,
											owner = nav.owner(screen),
											machineOwner = nav.owner(Screen.Machine(screen.machine)),
											onBack = { nav.pop() },
											onTabs = { nav.push(Screen.Editor(screen.machine, tabs = true)) },
											onHelp = { nav.push(Screen.Help) },
										)
									}
								}
								is Screen.Help -> HelpScreen(onBack = { nav.pop() })
								is Screen.Settings -> SettingsScreen(screen.machine, onBack = { nav.pop() }, onHelp = { nav.push(Screen.Help) })
							}
						}
					}
					// One confirmation for every action that may ask, over whatever screen asked.
					GuardDialog(guard)
					nav.untold?.let { ForgottenUntold(it) { nav.untold = null } }
				}
			}
		}
	}

	override fun onNewIntent(intent: Intent) {
		super.onNewIntent(intent)
		handle(intent)
	}

	override fun onStop() {
		super.onStop()
		// To the background: the next action asks again. A rotation isn't leaving.
		if (!isChangingConfigurations) forgetUnlock()
	}

	companion object {
		/** The computer to open, by its id: the home-screen widget's. */
		const val EXTRA_MACHINE = "io.github.elaizes.elamon.MACHINE"
	}

	/**
	 * elamon://pair?… from the QR code that `elamond pair` prints. Any web page can open one too,
	 * so it only ever shows the pairing screen, once, and waits for the user there.
	 */
	private fun handle(intent: Intent?) {
		// A widget's computer: its screen, over what was open, or back to it where it is open already.
		intent?.getStringExtra(EXTRA_MACHINE)?.let { id ->
			if (MachineStore.get(this).get(id) != null) nav.show(Screen.Machine(id))
			return
		}
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
 * The screen a fresh start opens over Home: the computer opened [last], while it is still among
 * the [paired] (reachable or not: its screen shows what it last had), unless the app was started
 * by a [link], whose pairing comes first. None otherwise: Home, as with nothing paired.
 */
fun startScreen(last: String?, paired: List<Paired>, link: Boolean): Screen? =
	last?.takeIf { id -> !link && paired.any { it.id == id } }?.let { Screen.Machine(it) }

/**
 * How the app looks on [top]: as the profile of the computer it shows says (a computer's screen,
 * or the settings opened from it), elsewhere as the first paired computer's, as last heard; with
 * none, as the built-in `default` profile. With the phone's [own] theme on, that theme over any of
 * them, the same everywhere: the one place it goes over a profile.
 */
fun prefsFor(top: Screen, paired: List<Paired>, profiles: Map<String, Profile>, own: OwnTheme? = null): Prefs {
	val id = when (top) {
		is Screen.Machine -> top.id
		is Screen.Editor -> top.machine
		is Screen.Settings -> top.machine ?: paired.firstOrNull()?.id
		else -> paired.firstOrNull()?.id
	}
	return (id?.let { profiles[it] }?.let(Prefs::of) ?: Prefs()).with(own)
}
