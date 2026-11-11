package io.github.elaizes.elamon.ui

import android.app.KeyguardManager
import android.content.Context
import android.os.Build
import android.os.SystemClock
import androidx.biometric.BiometricPrompt
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.res.stringResource
import androidx.core.content.ContextCompat
import androidx.fragment.app.FragmentActivity
import androidx.lifecycle.ViewModel
import kotlinx.coroutines.channels.BufferOverflow
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.receiveAsFlow
import io.github.elaizes.elamon.Before
import io.github.elaizes.elamon.Guarded
import io.github.elaizes.elamon.R
import io.github.elaizes.elamon.Settings
import io.github.elaizes.elamon.UnlockMemory
import io.github.elaizes.elamon.authenticators
import io.github.elaizes.elamon.data.Limits
import io.github.elaizes.elamon.data.withoutControls
import io.github.elaizes.elamon.gate

/** What an action shows before it runs: in the unlock prompt, or in the confirmation instead. */
data class Ask(
	val kind: Guarded,
	/** The prompt's title, naming the action and the computer: "Restart Test computer". */
	val prompt: String,
	/** The confirmation's title, "Restart the computer?", and its button, "Restart". */
	val question: String,
	val button: String,
	/** What follows from it, under the prompt's title and the confirmation's. */
	val body: String? = null,
	val icon: ImageVector? = null,
	/** Whether Confirm actions covers it: not for what was never confirmed, nor where a dialog has just asked. */
	val confirmable: Boolean = true,
)

/** An action waiting for the unlock prompt or the confirmation. */
data class Pending(val ask: Ask, val action: () -> Unit, val afterAsking: () -> Unit, val noLockScreen: Boolean = false) {
	fun run() {
		action()
		afterAsking()
	}
}

/** The guard's state, kept through a rotation like the prompt itself: the confirmation on screen, the action the prompt is for. */
class GuardState : ViewModel() {
	var confirming by mutableStateOf<Pending?>(null)
	var unlocking: Pending? = null
	val messages = Channel<String>(capacity = Limits.MESSAGES_KEPT, onBufferOverflow = BufferOverflow.DROP_OLDEST)
}

/** Unlocking and the missing screen lock, remembered for as long as the app runs. */
private object Unlocked {
	@Volatile var memory = UnlockMemory()
}

/**
 * The screen went off, or the app to the background: whatever the last unlock covered, the next
 * action asks again. A phone left unlocked in a pocket or handed over doesn't carry a grace along.
 */
fun forgetUnlock() {
	Unlocked.memory = Unlocked.memory.forgotten()
}

/** Whether the phone has a screen lock, without which it can't be unlocked for anything. */
fun hasScreenLock(context: Context): Boolean = context.getSystemService(KeyguardManager::class.java)?.isDeviceSecure == true

/**
 * Puts unlocking the phone, or a confirmation, before an action, as the settings and [gate] say.
 * Made in the activity's onCreate, as the prompt must be, to hear its answer even after a rotation.
 */
class Guard(private val activity: FragmentActivity, val state: GuardState) {
	private val prompt = BiometricPrompt(
		activity,
		ContextCompat.getMainExecutor(activity),
		object : BiometricPrompt.AuthenticationCallback() {
			override fun onAuthenticationSucceeded(result: BiometricPrompt.AuthenticationResult) {
				val p = take() ?: return
				// The grace that follows covers this kind and those below it.
				Unlocked.memory = Unlocked.memory.unlocked(p.ask.kind, SystemClock.elapsedRealtime())
				p.run()
			}

			// A finger or face not recognised: the prompt stays, for another try.
			override fun onAuthenticationFailed() {}

			override fun onAuthenticationError(errorCode: Int, errString: CharSequence) {
				val p = take() ?: return
				when (errorCode) {
					// Cancelled, or the app left: nothing is sent, and there's nothing to say.
					BiometricPrompt.ERROR_USER_CANCELED, BiometricPrompt.ERROR_NEGATIVE_BUTTON, BiometricPrompt.ERROR_CANCELED -> {}
					// The screen lock was removed meanwhile: as without one, confirming instead.
					BiometricPrompt.ERROR_NO_DEVICE_CREDENTIAL -> confirm(p.copy(noLockScreen = !Unlocked.memory.explained))
					else -> state.messages.trySend(errString.toString())
				}
			}
		},
	)

	/** Why unlocking failed, other than being cancelled, for a snackbar. */
	val messages: Flow<String> = state.messages.receiveAsFlow()

	/**
	 * Runs [action] once [ask] is answered: at once, after the confirmation, or after unlocking the
	 * phone. Cancelled, it doesn't run. [afterAsking] follows it where something was asked.
	 */
	fun run(asked: Ask, afterAsking: () -> Unit = {}, action: () -> Unit) {
		// The names in it (a process's, a computer's) come from afar, and the unlock prompt is the system's:
		// nothing in them may turn the words around.
		val ask = asked.copy(prompt = asked.prompt.withoutControls(), question = asked.question.withoutControls(), body = asked.body?.withoutControls(keepLineBreaks = true))
		val g = gate(ask.kind, ask.confirmable, Settings.get(activity).current, hasScreenLock(activity), SystemClock.elapsedRealtime(), Unlocked.memory)
		val p = Pending(ask, action, afterAsking, g.noLockScreen)
		when (g.before) {
			Before.Nothing -> action()
			Before.Confirm -> confirm(p)
			Before.Unlock -> unlock(p)
		}
	}

	/** The confirmation's button. */
	fun confirmed() {
		val p = state.confirming ?: return
		state.confirming = null
		p.run()
	}

	fun dismissed() {
		state.confirming = null
	}

	private fun confirm(p: Pending) {
		if (p.noLockScreen) Unlocked.memory = Unlocked.memory.copy(explained = true)
		state.confirming = p
	}

	private fun unlock(p: Pending) {
		state.unlocking = p
		val info = BiometricPrompt.PromptInfo.Builder()
			.setTitle(p.ask.prompt)
			.setDescription(p.ask.body)
			.setAllowedAuthenticators(authenticators(Build.VERSION.SDK_INT))
			.build()
		try {
			prompt.authenticate(info)
		} catch (e: Exception) {
			// Never the action without the unlock it waits for.
			take()
			state.messages.trySend(activity.getString(R.string.err_unknown))
		}
	}

	private fun take(): Pending? = state.unlocking.also { state.unlocking = null }
}

/** The activity's guard, for every action that may need it. */
val LocalGuard = staticCompositionLocalOf<Guard> { error("no Guard") }

/** The confirmation an action waits for, when it doesn't wait for unlocking. */
@Composable
fun GuardDialog(guard: Guard) {
	val p = guard.state.confirming ?: return
	val ask = p.ask
	GlassDialog(
		onDismiss = guard::dismissed,
		title = ask.question,
		icon = ask.icon,
		text = if (ask.body == null && !p.noLockScreen) null else {
			{
				Column(verticalArrangement = Arrangement.spacedBy(Space.m)) {
					ask.body?.let { Text(it) }
					if (p.noLockScreen) {
						Text(stringResource(R.string.unlock_no_lock_screen), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
					}
				}
			}
		},
		confirm = { TextButton(onClick = guard::confirmed) { Text(ask.button) } },
		dismiss = { TextButton(onClick = guard::dismissed) { Text(stringResource(R.string.cancel)) } },
	)
}
