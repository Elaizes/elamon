package ua.elamon

import android.content.Intent
import android.os.Bundle
import android.widget.Toast
import androidx.activity.ComponentActivity
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
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateListOf
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.toArgb
import androidx.core.graphics.drawable.toDrawable
import androidx.lifecycle.ViewModel
import androidx.lifecycle.ViewModelStore
import androidx.lifecycle.ViewModelStoreOwner
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import ua.elamon.data.PairLink
import ua.elamon.data.PairTarget
import ua.elamon.data.ServerStore
import ua.elamon.ui.ElamonTheme
import ua.elamon.ui.HelpScreen
import ua.elamon.ui.HelpTopic
import ua.elamon.ui.HomeScreen
import ua.elamon.ui.MachineScreen
import ua.elamon.ui.PairScreen
import ua.elamon.ui.SettingsScreen
import ua.elamon.ui.appColorScheme
import ua.elamon.ui.isDark

sealed interface Screen {
	data object Home : Screen
	data class Machine(val id: String) : Screen
	data class Pair(val target: PairTarget) : Screen
	data class Help(val topic: String? = null) : Screen
	data object Settings : Screen
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

class MainActivity : ComponentActivity() {
	private val nav: Nav by viewModels()

	override fun onCreate(savedInstanceState: Bundle?) {
		super.onCreate(savedInstanceState)
		enableEdgeToEdge()
		// The app's own background from the first frame, not the phone's day or night.
		val start = Settings.get(this).current
		window.setBackgroundDrawable(appColorScheme(this, start, isDark(this, start)).surface.toArgb().toDrawable())
		if (savedInstanceState == null) handle(intent)
		setContent {
			val prefs by Settings.get(this).state.collectAsStateWithLifecycle()
			ElamonTheme(prefs) {
				Surface(Modifier.fillMaxSize(), color = MaterialTheme.colorScheme.surface) {
					BackHandler(enabled = nav.stack.size > 1) { nav.pop() }
					AnimatedContent(
						nav.top,
						transitionSpec = {
							if (prefs.animations) (fadeIn(tween(240)) + scaleIn(tween(240), initialScale = 0.96f)) togetherWith fadeOut(tween(160))
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
								onSettings = { nav.push(Screen.Settings) },
							)
							is Screen.Machine -> {
								val server = ServerStore.get(this@MainActivity).get(screen.id)
								if (server == null) {
									LaunchedEffect(Unit) { nav.reset(Screen.Home) }
								} else {
									MachineScreen(
										server,
										owner = nav.owner(screen),
										onBack = { nav.pop() },
										onPairAgain = { nav.push(Screen.Pair(it)) },
										onForgotten = { nav.reset(Screen.Home) },
										onHelp = { nav.push(Screen.Help(it)) },
										onSettings = { nav.push(Screen.Settings) },
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
							Screen.Settings -> SettingsScreen(onBack = { nav.pop() }, onHelp = { nav.push(Screen.Help()) })
						}
					}
				}
			}
		}
	}

	override fun onNewIntent(intent: Intent) {
		super.onNewIntent(intent)
		handle(intent)
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
