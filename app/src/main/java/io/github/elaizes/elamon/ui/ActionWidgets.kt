package io.github.elaizes.elamon.ui

import android.content.res.Resources
import androidx.annotation.StringRes
import androidx.compose.animation.animateColorAsState
import androidx.compose.animation.core.AnimationSpec
import androidx.compose.animation.core.snap
import androidx.compose.animation.core.spring
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.selection.selectableGroup
import androidx.compose.foundation.selection.toggleable
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.FilterChip
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Slider
import androidx.compose.material3.Surface
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.MutableState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateMapOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonObject
import io.github.elaizes.elamon.ActionsState
import io.github.elaizes.elamon.Guarded
import io.github.elaizes.elamon.MachineViewModel
import io.github.elaizes.elamon.R
import io.github.elaizes.elamon.UnlockGrace
import io.github.elaizes.elamon.data.Action
import io.github.elaizes.elamon.data.ActionResult
import io.github.elaizes.elamon.data.Param
import io.github.elaizes.elamon.data.Params
import io.github.elaizes.elamon.data.Typed
import io.github.elaizes.elamon.data.UiDoc
import io.github.elaizes.elamon.data.Widget
import io.github.elaizes.elamon.data.WidgetState
import io.github.elaizes.elamon.data.bool
import io.github.elaizes.elamon.data.number
import io.github.elaizes.elamon.data.text

/**
 * An action whose open parameters the form asks for before it runs, and what has been given them so
 * far: kept with the request, so the form comes back after a rotation as it was left.
 */
class FormRequest(val action: Action, val fixed: JsonObject, val phrase: String, val button: String?, val key: String, val afterAsking: () -> Unit = {}) {
	/** The parameters asked for: those [fixed] leaves open. */
	val asked: List<Param> = Params.asked(action, fixed)

	/** Each asked parameter's value, as [Params.initial] starts it; null while nothing it takes is given. */
	val values = mutableStateMapOf<String, JsonElement?>().apply { for (p in asked) put(p.id, Params.initial(p)) }

	/** What is typed into the fields that are typed in, as typed. */
	val texts = mutableStateMapOf<String, String>().apply { for (p in asked) put(p.id, Params.initial(p)?.text().orEmpty()) }

	/** The values the action runs with: the fixed ones and those given. */
	fun params(): JsonObject = buildJsonObject {
		fixed.forEach { (k, v) -> put(k, v) }
		for (p in asked) values[p.id]?.takeIf { it !is JsonNull }?.let { put(p.id, it) }
	}
}

/**
 * Runs the computer's actions from widgets: asks for the parameters a widget doesn't fix, then
 * puts the confirmation or the unlock before it as the action's `confirm` and `unlock` and the
 * profile's grace say ([Guard]); then [run]s it with its parameters, its key and the text typed. The
 * same for a computer's screen, its card on the home screen and a Quick Settings tile.
 */
class ActionLauncher(
	private val guard: Guard,
	private val run: (action: Action, params: JsonObject, key: String, typed: String?) -> Unit,
	val ui: UiDoc,
	/** The computer's name, for the unlock prompt's title. */
	private val computer: String,
	private val res: Resources,
	private val tap: () -> Unit,
	val form: MutableState<FormRequest?>,
	val running: ActionsState,
) {
	private val grace = UnlockGrace.of(ui.profile.unlock.grace)

	fun action(id: String?): Action? = ui.action(id)

	/** Whether [action] can run now: the computer has it for this phone, control is on, and the widget isn't disabled. */
	fun enabled(action: Action?, state: WidgetState?): Boolean = action != null && ui.control.enabled && state?.enabled != false

	/**
	 * Runs [action] with the [fixed] values, [phrase] naming it in what asks ("Restart", "Quit
	 * firefox"); [key] tells this run apart, [afterAsking] follows once something was asked.
	 */
	fun launch(action: Action, fixed: JsonObject, phrase: String, key: String = action.id, button: String? = null, afterAsking: () -> Unit = {}) {
		tap()
		if (Params.asked(action, fixed).isNotEmpty()) {
			form.value = FormRequest(action, fixed, phrase, button, key, afterAsking)
			return
		}
		go(action, fixed, phrase, button, key, afterAsking)
	}

	/** What the form gave, run; what asks next names it, as a select's choice is named ("Mode: Fast?"). */
	fun submit(req: FormRequest, params: JsonObject) {
		form.value = null
		go(req.action, params, phraseWith(req.phrase, req.asked, params), req.button, req.key, req.afterAsking)
	}

	private fun go(action: Action, params: JsonObject, phrase: String, button: String?, key: String, afterAsking: () -> Unit) {
		val typed = Params.typedText(action, params)
		guard.run(ask(action, params, phrase, button), afterAsking) { run(action, params, key, typed) }
	}

	/** What the confirmation and the unlock prompt say: 0.5's words for power and processes, the phrase's for the rest. */
	fun ask(action: Action, params: JsonObject, phrase: String, button: String? = null): Ask {
		val power = POWER[action.id]
		val kill = params["signal"].text() == "kill"
		return Ask(
			Guarded.of(action.kind),
			prompt = power?.let { res.getString(it.unlock, computer) } ?: res.getString(R.string.action_prompt, phrase, computer),
			question = power?.let { res.getString(it.question) } ?: res.getString(R.string.action_question, phrase),
			button = button ?: phrase.substringBefore(PHRASE_SEPARATOR).ifEmpty { action.name },
			body = when {
				power != null -> res.getString(power.explain)
				action.id == SIGNAL -> res.getString(if (kill) R.string.processes_force_kill_body else R.string.processes_quit_body)
				else -> null
			},
			icon = action.icon,
			confirm = action.confirm,
			unlock = action.unlock,
			grace = grace,
			typed = Params.typedText(action, params),
			typedKind = Params.typedKind(action, params) ?: Typed.Command,
		)
	}

	private class Words(@StringRes val question: Int, @StringRes val explain: Int, @StringRes val unlock: Int)

	private companion object {
		const val SIGNAL = "process.signal"
		val POWER = mapOf(
			"power.lock" to Words(R.string.power_lock_ask, R.string.power_lock_explain, R.string.unlock_lock),
			"power.suspend" to Words(R.string.power_sleep_ask, R.string.power_sleep_explain, R.string.unlock_sleep),
			"power.reboot" to Words(R.string.power_restart_ask, R.string.power_restart_explain, R.string.unlock_restart),
			"power.poweroff" to Words(R.string.power_shut_down_ask, R.string.power_shut_down_explain, R.string.unlock_shut_down),
		)
	}
}

/** Between a widget's label and the value it sets, in what asks: "Power profile: Performance". */
const val PHRASE_SEPARATOR = ": "

/**
 * [phrase] with the values [params] give the [asked] parameters, as what asks names a select's
 * choice: "Mode: Fast", "Fan: 50%"; with several, each after its name ("Backup: Target Disk, Verify
 * yes"). As it is when none of them has a value to name.
 */
internal fun phraseWith(phrase: String, asked: List<Param>, params: JsonObject): String {
	val named = asked.mapNotNull { p -> valueWords(p, params[p.id])?.let { p to it } }
	if (named.isEmpty()) return phrase
	val words = if (asked.size == 1) named.single().second else named.joinToString(", ") { (p, v) -> "${p.label ?: p.id} $v" }
	return phrase + PHRASE_SEPARATOR + words
}

/**
 * What asks before a toggle sets [p] to [value]: its [label] and the choice it turns to ("DP-1:
 * Off"), as a select's; the label alone where the choice is called the same, or isn't one.
 */
internal fun togglePhrase(label: String, p: Param?, value: JsonElement): String {
	if (p?.kind != "choice") return label
	val words = valueWords(p, value) ?: return label
	return if (words.equals(label.trim(), ignoreCase = true)) label else label + PHRASE_SEPARATOR + words
}

/** A value of [p] as it reads: a choice by its label, a number with its unit, a switch yes or no; null for none. */
internal fun valueWords(p: Param, v: JsonElement?): String? {
	if (v == null || v is JsonNull) return null
	return when (p.kind) {
		"choice" -> v.text()?.let { t -> p.choices?.firstOrNull { it.value == t }?.label ?: t }
		"bool" -> v.bool()?.let(Fmt::bool)
		"range", "integer", "number" -> v.number()?.let { n -> Fmt.withUnit(Fmt.round(n, if (n % 1.0 == 0.0) 0 else 1), p.unit) }
		else -> v.text()?.ifEmpty { null }
	}
}

/** Power off and restart are drawn in red: they end what runs. */
private val DANGER = setOf("power.poweroff", "power.reboot")

/**
 * Runs an action, or opens a dashboard: a row, its icon before its name; where the cell is too
 * narrow for both (half a phone's width, or large text) and as a group's narrowest, a tile with its
 * icon over its name. In a group, on the group's card rather than one of its own.
 */
@Composable
fun ButtonWidget(scope: DashScope, w: Widget, modifier: Modifier) {
	val s = scope.state(w)
	val a = scope.actions
	val action = a.action(w.action)
	// One whose action this phone may not run isn't described at all; one that only opens always works.
	val enabled = if (w.action != null) a.enabled(action, s) else true
	val label = w.label ?: action?.name.orEmpty()
	val run: () -> Unit = {
		if (action != null) a.launch(action, w.params ?: JsonObject(emptyMap()), label)
		else w.open?.let(scope.open)
	}
	val running = action?.id in a.running.running
	val result = action?.let { a.running.results[it.id] }
	val alpha = if (enabled) 1f else Alpha.DISABLED
	val mark = icon(w.icon ?: action?.icon)
	// The spinner in the icon's place while it runs, how it ended under the name: a pill beside the
	// name left it a few letters a line.
	val ended = outcome(running, result, scope.vm, action)
	val click = Modifier.clickable(enabled = takesTap(enabled, running), role = Role.Button, onClick = run)
	if (LocalDepth.current == 0) {
		val accent = if (w.color != null) accent(w.color) else MaterialTheme.colorScheme.secondary
		CardSurface(modifier) {
			if (narrowCell(TextSizes.rowLeast)) {
				ControlTile(label, captionOf(w, s), click, alpha, icon = { BusyBadge(mark, accent.copy(alpha = alpha), running) }, below = ended)
			} else {
				Column(Modifier.padding(vertical = Space.xs), verticalArrangement = Arrangement.Center) {
					ListRow(label, captionOf(w, s), mark, accent, enabled = takesTap(enabled, running), onClick = run, busy = running, below = ended)
				}
			}
		}
		return
	}
	val danger = action?.id in DANGER
	CardSurface(modifier, color = if (danger) Metric.colors.hot.copy(alpha = Alpha.DANGER) else cardColor()) {
		if (w.span <= 1 || narrowCell(TextSizes.innerRowLeast)) {
			ControlTile(label, null, click, alpha, padding = Space.xl, icon = {
				if (running) Spinner(Sizes.icon)
				else Icon(mark, null, tint = (if (danger) Metric.colors.hot else MaterialTheme.colorScheme.onSurface).copy(alpha = alpha))
			}, below = ended)
		} else {
			Column(click.padding(Space.m), verticalArrangement = Arrangement.spacedBy(Space.s, Alignment.CenterVertically)) {
				Row(verticalAlignment = Alignment.CenterVertically) {
					if (running) Spinner()
					else Icon(if (w.open != null && action == null) icon(w.icon) else ElIcons.Play, null, tint = accent(w.color ?: "tertiary").copy(alpha = alpha), modifier = Modifier.size(Sizes.iconM))
					Spacer(Modifier.width(Space.s))
					Text(label, style = MaterialTheme.typography.titleSmall, color = MaterialTheme.colorScheme.onSurface.copy(alpha = alpha), maxLines = 2, overflow = TextOverflow.Ellipsis, modifier = Modifier.weight(1f).wholeWords())
				}
				ended?.invoke()
			}
		}
	}
}

/**
 * A control as a tile, all in the middle: its icon, its name and a line about it from the top, and at
 * the bottom what it sets or how it ended, so a row of them has each on one line.
 */
@Composable
private fun ControlTile(
	label: String,
	caption: String?,
	modifier: Modifier,
	alpha: Float,
	padding: Dp = Space.card,
	icon: (@Composable () -> Unit)? = null,
	below: (@Composable () -> Unit)? = null,
) {
	Footed(
		modifier.fillMaxWidth().padding(vertical = padding, horizontal = Space.s),
		gap = Space.s,
		main = {
			Column(Modifier.fillMaxWidth(), horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(Space.s)) {
				icon?.invoke()
				Text(label, style = MaterialTheme.typography.labelLarge, color = MaterialTheme.colorScheme.onSurface.copy(alpha = alpha), textAlign = TextAlign.Center, maxLines = 2, overflow = TextOverflow.Ellipsis, modifier = Modifier.wholeWords())
				if (caption != null) Text(caption, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant.copy(alpha = alpha), textAlign = TextAlign.Center, maxLines = 2, overflow = TextOverflow.Ellipsis, modifier = Modifier.wholeWords())
			}
		},
		last = { if (below != null) Box(Modifier.fillMaxWidth(), contentAlignment = Alignment.Center) { below() } },
	)
}

/** Whether a button takes a tap: its action may run and isn't running already, or a second tap would ask again for nothing. */
internal fun takesTap(enabled: Boolean, running: Boolean): Boolean = enabled && !running

/** How a button's action last ended, shown under its name: none while it runs again, the spinner in its icon's place saying so. */
internal fun shownResult(running: Boolean, result: ActionResult?): ActionResult? = result?.takeUnless { running }

/** What a button shows under its name: how the command ended ([shownResult]), its output a tap away. */
private fun outcome(running: Boolean, result: ActionResult?, vm: MachineViewModel, action: Action?): (@Composable () -> Unit)? {
	val r = shownResult(running, result) ?: return null
	if (action == null) return null
	return { ActionOutcome(r, Modifier.clip(PillShape).clickable(role = Role.Button) { vm.showActionResult(action.id) }) }
}

@Composable
fun ActionOutcome(r: ActionResult, modifier: Modifier = Modifier) {
	when {
		r.timedOut -> Pill(stringResource(R.string.control_timed_out), Metric.colors.warn, modifier, dot = true)
		r.exitCode == null || r.exitCode == 0 -> Pill(stringResource(R.string.control_done), Metric.colors.good, modifier, dot = true)
		else -> Pill(stringResource(R.string.control_exit_code, r.exitCode.toString()), Metric.colors.hot, modifier, dot = true)
	}
}

/** What a command printed, opened when it is done. */
@Composable
fun ActionOutput(vm: MachineViewModel, ui: UiDoc?, state: ActionsState) {
	val id = state.shown ?: return
	val r = state.results[id] ?: return
	val name = ui?.action(id)?.name ?: id
	GlassSheet(onDismiss = { vm.showActionResult(null) }) {
		Row(verticalAlignment = Alignment.CenterVertically) {
			Text(name, style = MaterialTheme.typography.headlineSmall, modifier = Modifier.weight(1f).wholeWords())
			Spacer(Modifier.width(Space.s))
			ActionOutcome(r)
		}
		Surface(shape = InnerShape, color = MaterialTheme.colorScheme.surfaceContainerHighest, modifier = Modifier.fillMaxWidth().heightIn(max = Sizes.outputMax)) {
			SelectionContainer {
				Text(
					r.output?.ifBlank { null } ?: stringResource(R.string.control_no_output),
					fontFamily = FontFamily.Monospace,
					style = MaterialTheme.typography.bodySmall,
					modifier = Modifier.verticalScroll(rememberScrollState()).padding(Space.m),
				)
			}
		}
	}
}

/** A switch: the action's [Widget.param] set to its `on` or `off` value. */
@Composable
fun ToggleWidget(scope: DashScope, w: Widget, modifier: Modifier) {
	val s = scope.state(w)
	val a = scope.actions
	val action = a.action(w.action)
	val on = w.on ?: JsonPrimitive(true)
	val off = w.off ?: JsonPrimitive(false)
	val checked = toggledOn(w, s)
	val label = w.label ?: action?.name.orEmpty()
	val enabled = a.enabled(action, s) && w.param != null
	val set: (Boolean) -> Unit = { v ->
		if (w.param != null && action != null) {
			val value = if (v) on else off
			a.launch(action, withParam(w.params, w.param, value), togglePhrase(label, action.params.find { it.id == w.param }, value))
		}
	}
	val padding = if (LocalDepth.current > 0) Space.none else Space.xs
	CardSurface(modifier) {
		if (narrowCell(TextSizes.rowLeast)) {
			ControlTile(
				label, captionOf(w, s),
				Modifier.toggleable(value = checked, enabled = enabled, role = Role.Switch, onValueChange = set),
				if (enabled) 1f else Alpha.DISABLED,
				below = { Switch(checked = checked, onCheckedChange = null, enabled = enabled) },
			)
		} else {
			Column(Modifier.padding(vertical = padding), verticalArrangement = Arrangement.Center) { SwitchRow(label, captionOf(w, s), checked, enabled, onChange = set) }
		}
	}
}

/** Whether two JSON values are the same value: a number as a number, whatever its writing. */
fun sameValue(a: JsonElement, b: JsonElement): Boolean {
	val na = a.number()
	val nb = b.number()
	if (na != null && nb != null) return na == nb
	val ba = a.bool()
	val bb = b.bool()
	if (ba != null && bb != null) return ba == bb
	return a.text() == b.text() && (a as? JsonPrimitive)?.isString == (b as? JsonPrimitive)?.isString
}

/** A widget's fixed [params] and [param] set to [value]. */
private fun withParam(params: JsonObject?, param: String, value: JsonElement): JsonObject = buildJsonObject {
	params?.forEach { (k, v) -> put(k, v) }
	put(param, value)
}

/** Whether the toggle [w] is on in the state [s]: its value is its `on`; off while it has none. */
fun toggledOn(w: Widget, s: WidgetState?): Boolean = s?.v != null && sameValue(s.v, w.on ?: JsonPrimitive(true))

/** What a tap on a button or a toggle runs away from its dashboard (a card's quick action, a tile): its action's values, and the phrase that names it. */
data class Tap(val params: JsonObject, val phrase: String)

/**
 * A tap on [w], a `button` or a `toggle` running [action], where it isn't drawn as on its dashboard:
 * a button's fixed values; a toggle turned to its other value, off while [on] (its state, as
 * [toggledOn] reads it) and on otherwise, as its switch would be. Its label, or the action's name.
 */
fun tapOf(w: Widget, action: Action, on: Boolean): Tap {
	val label = w.label ?: action.name
	val param = w.param
	if (w.type != "toggle" || param == null) return Tap(w.params ?: JsonObject(emptyMap()), label)
	val value = if (on) w.off ?: JsonPrimitive(false) else w.on ?: JsonPrimitive(true)
	return Tap(withParam(w.params, param, value), togglePhrase(label, action.params.find { it.id == param }, value))
}

/** One of an action's choices, side by side, the one now marked: as 0.5's power profiles. */
@Composable
fun SelectWidget(scope: DashScope, w: Widget, modifier: Modifier) {
	val s = scope.state(w)
	val a = scope.actions
	val action = a.action(w.action)
	val param = action?.params?.find { it.id == w.param }
	val choices = param?.choices.orEmpty()
	val enabled = a.enabled(action, s)
	val now = s?.v?.text()
	val label = w.label ?: action?.name.orEmpty()
	// The highlight moves to the choice taken, or with animations off is there at once.
	val move: AnimationSpec<Color> = if (LocalPrefs.current.animations) spring() else snap()
	val least = with(LocalDensity.current) { TextSizes.choiceLeast.toDp() }
	val most = ((LocalCellWidth.current - Space.card * 2 + Space.inner) / (least + Space.inner)).coerceIn(1f, CHOICES_IN_A_ROW.toFloat()).toInt()
	Section(label, icon(w.icon), modifier, accent = accent(w.color)) {
		captionOf(w, s)?.let { Text(it, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant) }
		EvenRows(choices, most, Modifier.selectableGroup()) { c ->
			val on = c.value == now
			val bg by animateColorAsState(if (on) MaterialTheme.colorScheme.primaryContainer else cardColor(), move, label = "choice")
			val fg = (if (on) MaterialTheme.colorScheme.onPrimaryContainer else MaterialTheme.colorScheme.onSurfaceVariant).copy(alpha = if (enabled) 1f else Alpha.DISABLED)
			val name = c.label ?: c.value
			Column(
				Modifier.heightIn(min = Sizes.touchTarget).clip(InnerShape).background(bg)
					.selectable(selected = on, enabled = enabled && !on, role = Role.Button) {
						if (action != null && w.param != null) a.launch(action, withParam(w.params, w.param, JsonPrimitive(c.value)), label + PHRASE_SEPARATOR + name)
					}
					.padding(vertical = Space.m, horizontal = Space.s),
				horizontalAlignment = Alignment.CenterHorizontally,
				verticalArrangement = Arrangement.spacedBy(Space.s, Alignment.CenterVertically),
			) {
				if (action?.id == PROFILE_SET) Icon(profileIcon(c.value), null, tint = fg)
				// One line: a name too long for its tile ("Performance" at a large font) gets smaller,
				// rather than breaking inside the word.
				Text(name, style = MaterialTheme.typography.labelMedium, color = fg, maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.shrinkToFit())
			}
		}
	}
}

/** The most choices to a row. */
private const val CHOICES_IN_A_ROW = 3
private const val PROFILE_SET = "profile.set"

/** The power profiles' pictures, as 0.5 drew them. */
@Composable private fun profileIcon(p: String): ImageVector = when (p) {
	"power-saver", "low-power", "quiet", "cool" -> ElIcons.Leaf
	"performance" -> ElIcons.Rocket
	else -> ElIcons.Gauge
}

/** A number in an action's range, on its step, sent when the finger lets go. */
@Composable
fun SliderWidget(scope: DashScope, w: Widget, modifier: Modifier) {
	val s = scope.state(w)
	val a = scope.actions
	val action = a.action(w.action)
	val param = action?.params?.find { it.id == w.param }
	val min = param?.min ?: 0.0
	val max = (param?.max ?: Thresholds.PERCENT.toDouble()).coerceAtLeast(min + 1)
	val now = s?.number ?: param?.default.number() ?: min
	var dragging by remember { mutableStateOf<Float?>(null) }
	val shown = dragging?.toDouble() ?: now
	val label = w.label ?: action?.name.orEmpty()
	val unit = param?.unit
	val text = Fmt.withUnit(Fmt.round(shown, if (param?.step?.let { it % 1.0 == 0.0 } != false) 0 else 1), unit)
	Section(
		label,
		icon(w.icon),
		modifier,
		accent = accent(w.color),
		trailing = { Text(text, style = MaterialTheme.typography.titleMedium) },
	) {
		captionOf(w, s)?.let { Text(it, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant) }
		Slider(
			value = shown.toFloat(),
			// Without ticks it slides freely: what it shows is put on the step.
			onValueChange = { v -> dragging = param?.let { Params.snap(v.toDouble(), it).toFloat() } ?: v },
			onValueChangeFinished = {
				val v = dragging
				dragging = null
				if (v != null && action != null && param != null) {
					val snapped = Params.snap(v.toDouble(), param)
					a.launch(action, withParam(w.params, param.id, Params.number(snapped, param)), label + PHRASE_SEPARATOR + Fmt.withUnit(Fmt.round(snapped, if (snapped % 1.0 == 0.0) 0 else 1), unit))
				}
			},
			valueRange = min.toFloat()..max.toFloat(),
			steps = sliderSteps(min, max, param?.step),
			enabled = a.enabled(action, s) && param != null,
		)
	}
}

/** Past this many steps a slider is drawn without its ticks, and its value put on the step as it slides. */
private const val MAX_STEPS = 100

/** The ticks between a slider's ends for [step] from [min] to [max]: none past [MAX_STEPS], so it reaches every value. */
internal fun sliderSteps(min: Double, max: Double, step: Double?): Int =
	step?.takeIf { it > 0 }?.let { (((max - min) / it).toInt() - 1).coerceAtLeast(0) }?.takeIf { it <= MAX_STEPS } ?: 0

/**
 * The values an action needs that the widget leaves open: a choice among its choices, a number on
 * its range's steps, a switch, or typed; run once each takes its value.
 */
@OptIn(ExperimentalLayoutApi::class)
@Composable
fun ParamForm(req: FormRequest, onDismiss: () -> Unit, onDone: (JsonObject) -> Unit) {
	val asked = req.asked
	val values = req.values
	val texts = req.texts
	val params = req.params()
	val ok = Params.complete(req.action, params)
	val title = req.action.name.ifEmpty { req.phrase }
	GlassDialog(
		onDismiss = onDismiss,
		title = title,
		icon = icon(req.action.icon),
		text = {
			Column(verticalArrangement = Arrangement.spacedBy(Space.m)) {
				for (p in asked) {
					val name = p.label ?: p.id
					when (p.kind) {
						"choice" -> {
							// The one choice asked, named as the action is ("Mode" under "Mode"): the title says it.
							if (!Params.namedByTitle(asked, p, title)) Text(name, style = MaterialTheme.typography.labelLarge, color = MaterialTheme.colorScheme.primary)
							FlowRow(horizontalArrangement = Arrangement.spacedBy(Space.s)) {
								for (c in p.choices.orEmpty()) {
									FilterChip(selected = values[p.id]?.text() == c.value, onClick = { values[p.id] = JsonPrimitive(c.value) }, label = { Text(c.label ?: c.value) }, shape = PillShape)
								}
							}
						}
						"bool" -> Row(verticalAlignment = Alignment.CenterVertically) {
							Text(name, style = MaterialTheme.typography.bodyLarge, modifier = Modifier.weight(1f))
							Spacer(Modifier.width(Space.m))
							Switch(checked = values[p.id].bool() == true, onCheckedChange = { values[p.id] = JsonPrimitive(it) })
						}
						"range" -> RangeField(p, name, values[p.id].number()) { values[p.id] = Params.number(it, p) }
						else -> OutlinedTextField(
							value = texts[p.id].orEmpty(),
							onValueChange = { t ->
								texts[p.id] = t
								values[p.id] = when (p.kind) {
									"integer" -> t.trim().toLongOrNull()?.let { JsonPrimitive(it) }
									"number" -> t.trim().replace(',', '.').toDoubleOrNull()?.let { JsonPrimitive(it) }
									else -> JsonPrimitive(t)
								}
							},
							label = { Text(listOfNotNull(name, p.unit).joinToString(" · ")) },
							isError = !Params.valid(p, values[p.id]),
							singleLine = true,
							keyboardOptions = KeyboardOptions(keyboardType = if (p.kind == "integer" || p.kind == "number") KeyboardType.Decimal else KeyboardType.Text),
							modifier = Modifier.fillMaxWidth(),
						)
					}
				}
			}
		},
		confirm = DialogButton(stringResource(R.string.control_continue), enabled = ok) { onDone(params) },
		dismiss = DialogButton(stringResource(R.string.cancel), onClick = onDismiss),
	)
}

@Composable
private fun RangeField(p: Param, name: String, value: Double?, onChange: (Double) -> Unit) {
	val min = p.min ?: 0.0
	val max = (p.max ?: Thresholds.PERCENT.toDouble()).coerceAtLeast(min + 1)
	val v = value ?: min
	Column {
		Row {
			Text(name, style = MaterialTheme.typography.bodyLarge, modifier = Modifier.weight(1f))
			Text(Fmt.withUnit(Fmt.round(v, if (v % 1.0 == 0.0) 0 else 1), p.unit), style = MaterialTheme.typography.titleSmall)
		}
		Slider(value = v.toFloat(), onValueChange = { onChange(Params.snap(it.toDouble(), p)) }, valueRange = min.toFloat()..max.toFloat(), steps = sliderSteps(min, max, p.step))
	}
}

/** Control is off on the computer: why, and how to turn it on there. Action widgets stay, disabled. */
@Composable
fun ControlOff(reason: String?) {
	val (body, code) = when (reason) {
		"held_off" -> R.string.control_held_off_body to "elamond control on"
		"settings_unreadable" -> R.string.control_unreadable_body to "elamond check"
		else -> R.string.control_off_body to "control = true"
	}
	Banner(ElIcons.Lock, MaterialTheme.colorScheme.secondary, stringResource(body), title = stringResource(R.string.control_off)) {
		CodeBlock(code)
		if (reason != "held_off" && reason != "settings_unreadable") {
			Text(stringResource(R.string.control_off_where), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
		}
	}
}

fun Color.disabled(enabled: Boolean): Color = if (enabled) this else copy(alpha = Alpha.DISABLED)
