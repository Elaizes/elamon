package ua.elamon

import android.content.Intent
import android.os.Bundle
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
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateListOf
import androidx.compose.ui.Modifier
import androidx.lifecycle.ViewModel
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import ua.elamon.data.PairTarget
import ua.elamon.data.ServerStore
import ua.elamon.ui.ElamonTheme
import ua.elamon.ui.HelpScreen
import ua.elamon.ui.HelpTopic
import ua.elamon.ui.HomeScreen
import ua.elamon.ui.MachineScreen
import ua.elamon.ui.PairScreen
import ua.elamon.ui.SettingsScreen

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

	fun push(s: Screen) {
		stack.add(s)
	}

	fun pop() {
		if (stack.size > 1) stack.removeAt(stack.lastIndex)
	}

	/** Replaces everything above Home, e.g. after pairing. */
	fun reset(s: Screen) {
		while (stack.size > 1) stack.removeAt(stack.lastIndex)
		if (s != Screen.Home) stack.add(s)
	}
}

class MainActivity : ComponentActivity() {
	private val nav: Nav by viewModels()

	override fun onCreate(savedInstanceState: Bundle?) {
		super.onCreate(savedInstanceState)
		enableEdgeToEdge()
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
								onBack = { nav.pop() },
								onPaired = { nav.reset(Screen.Machine(it.id)) },
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

	/** elamon://pair?… from the QR code that `elamond pair` prints. */
	private fun handle(intent: Intent?) {
		val uri = intent?.data ?: return
		val target = PairTarget.fromUri(uri) ?: return
		nav.push(Screen.Pair(target))
	}
}
