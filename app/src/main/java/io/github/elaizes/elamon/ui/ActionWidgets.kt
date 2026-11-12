package io.github.elaizes.elamon.ui

import android.content.res.Resources
import androidx.annotation.StringRes
import androidx.compose.animation.animateColorAsState
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.selection.selectableGroup
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.FilterChip
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Slider
import androidx.compose.material3.Surface
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.MutableState
import androidx.compose.runtime.compositionLocalOf
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
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.style.TextOverflow
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
import io.github.elaizes.elamon.data.UiDoc
import io.github.elaizes.elamon.data.Widget
import io.github.elaizes.elamon.data.WidgetState
import io.github.elaizes.elamon.data.bool
import io.github.elaizes.elamon.data.number
import io.github.elaizes.elamon.data.text

/** An action whose open parameters the form asks for before it runs. */
data class FormRequest(val action: Action, val fixed: JsonObject, val phrase: String, val button: String?, val key: String, val afterAsking: () -> Unit = {})

/**
 * Runs the computer's actions from widgets: asks for the parameters a widget doesn't fix, then
 * puts the confirmation or the unlock before it as the action's `confirm` and `unlock` and the
 * profile's grace say ([Guard]).
 */
class ActionLauncher(
	private val guard: Guard,
	private val vm: MachineViewModel,
	val ui: UiDoc,
	/** The computer's name, for the unlock prompt's title. */
	private val computer: String,
	private val res: Resources,
	private val tap: () -> Unit,
	/** The form on screen, if any. */
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

	/** What the form gave, run. */
	fun submit(req: FormRequest, params: JsonObject) {
		form.value = null
		go(req.action, params, req.phrase, req.button, req.key, req.afterAsking)
	}

	private fun go(action: Action, params: JsonObject, phrase: String, button: String?, key: String, afterAsking: () -> Unit) {
		guard.run(ask(action, params, phrase, button), afterAsking) { vm.run(action, params, key) }
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
			icon = icon(action.icon),
			confirm = action.confirm,
			unlock = action.unlock,
			grace = grace,
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

/** Inside a group: buttons are drawn as its tiles and rows rather than as cards of their own. */
val LocalInGroup = compositionLocalOf { false }

/** Power off and restart are drawn in red: they end what runs. */
private val DANGER = setOf("power.poweroff", "power.reboot")

/** Tiles inside a card: choices, power, actions. */
private val InnerTileShape = RoundedCornerShape(Radius.lPlus)

/**
 * Runs an action, or opens a dashboard: in a group, a tile (span 1, as the power buttons) or a row
 * (as the settings' actions); alone, a row in a card of its own.
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
	if (!LocalInGroup.current) {
		ListGroup(modifier) {
			ListRow(
				label,
				summary = captionOf(w, s),
				icon = icon(w.icon ?: action?.icon),
				accent = if (w.color != null) accent(w.color) else MaterialTheme.colorScheme.secondary,
				enabled = enabled,
				onClick = run,
				trailing = outcome(running, result, scope.vm, action),
			)
		}
		return
	}
	if (w.span <= 1) {
		val danger = action?.id in DANGER
		Column(
			modifier.heightIn(min = Sizes.powerTileMin).clip(InnerTileShape)
				.background(if (danger) Metric.colors.hot.copy(alpha = Alpha.DANGER) else MaterialTheme.colorScheme.surfaceContainerHigh)
				.clickable(enabled = enabled && !running, role = Role.Button, onClick = run)
				.padding(vertical = Space.l, horizontal = Space.xs),
			horizontalAlignment = Alignment.CenterHorizontally,
			verticalArrangement = Arrangement.spacedBy(Space.xsPlus, Alignment.CenterVertically),
		) {
			if (running) CircularProgressIndicator(Modifier.size(Sizes.iconL), strokeWidth = Sizes.spinnerStroke)
			else Icon(icon(w.icon ?: action?.icon), null, tint = (if (danger) Metric.colors.hot else MaterialTheme.colorScheme.onSurface).copy(alpha = alpha))
			Text(label, style = MaterialTheme.typography.labelLarge, color = MaterialTheme.colorScheme.onSurface.copy(alpha = alpha), maxLines = 2, overflow = TextOverflow.Ellipsis)
		}
		return
	}
	Surface(shape = InnerTileShape, color = MaterialTheme.colorScheme.surfaceContainerHigh, modifier = modifier.fillMaxWidth()) {
		Row(
			Modifier.clickable(enabled = enabled && !running, role = Role.Button, onClick = run).padding(horizontal = Space.l, vertical = Space.mPlus),
			verticalAlignment = Alignment.CenterVertically,
		) {
			Icon(if (w.open != null && action == null) icon(w.icon) else ElIcons.Play, null, tint = accent(w.color ?: "tertiary").copy(alpha = alpha), modifier = Modifier.size(Sizes.iconM))
			Spacer(Modifier.width(Space.m))
			Text(label, style = MaterialTheme.typography.titleSmall, color = MaterialTheme.colorScheme.onSurface.copy(alpha = alpha), modifier = Modifier.weight(1f), maxLines = 2, overflow = TextOverflow.Ellipsis)
			outcome(running, result, scope.vm, action)?.invoke()
		}
	}
}

/** What a button shows at its end: a spinner while its action runs, then how the command ended, its output a tap away. */
private fun outcome(running: Boolean, result: ActionResult?, vm: MachineViewModel, action: Action?): (@Composable () -> Unit)? = when {
	running -> { { CircularProgressIndicator(Modifier.size(Sizes.iconM), strokeWidth = Sizes.spinnerStroke) } }
	result != null && action != null -> { { ActionOutcome(result, Modifier.clip(PillShape).clickable(role = Role.Button) { vm.showActionResult(action.id) }) } }
	else -> null
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
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun ActionOutput(vm: MachineViewModel, ui: UiDoc?, state: ActionsState) {
	val id = state.shown ?: return
	val r = state.results[id] ?: return
	val name = ui?.action(id)?.name ?: id
	ModalBottomSheet(onDismissRequest = { vm.showActionResult(null) }, containerColor = MaterialTheme.colorScheme.surfaceContainerLow) {
		Column(Modifier.padding(start = Space.xxl, end = Space.xxl, bottom = Space.xxxl), verticalArrangement = Arrangement.spacedBy(Space.m)) {
			Row(verticalAlignment = Alignment.CenterVertically) {
				Text(name, style = MaterialTheme.typography.headlineSmall, modifier = Modifier.weight(1f))
				ActionOutcome(r)
			}
			Surface(shape = RoundedCornerShape(Radius.mPlus), color = MaterialTheme.colorScheme.surfaceContainerHighest, modifier = Modifier.fillMaxWidth().heightIn(max = Sizes.outputMax)) {
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
}

/** A switch: the action's [Widget.param] set to its `on` or `off` value. */
@Composable
fun ToggleWidget(scope: DashScope, w: Widget, modifier: Modifier) {
	val s = scope.state(w)
	val a = scope.actions
	val action = a.action(w.action)
	val on = w.on ?: JsonPrimitive(true)
	val off = w.off ?: JsonPrimitive(false)
	val checked = s?.v != null && sameValue(s.v, on)
	val label = w.label ?: action?.name.orEmpty()
	ListGroup(modifier) {
		SwitchRow(label, captionOf(w, s), checked, enabled = a.enabled(action, s) && w.param != null) { v ->
			val param = w.param ?: return@SwitchRow
			action ?: return@SwitchRow
			a.launch(action, withParam(w.params, param, if (v) on else off), label)
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

/** One of an action's choices, side by side, the one now marked: as 0.5's power profiles. */
@OptIn(ExperimentalLayoutApi::class)
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
	Section(label, icon(w.icon), modifier, accent = accent(w.color)) {
		captionOf(w, s)?.let { Text(it, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant) }
		FlowRow(
			Modifier.fillMaxWidth().selectableGroup(),
			horizontalArrangement = Arrangement.spacedBy(Space.s),
			verticalArrangement = Arrangement.spacedBy(Space.s),
			maxItemsInEachRow = CHOICES_IN_A_ROW,
		) {
			for (c in choices) {
				val on = c.value == now
				val bg by animateColorAsState(if (on) MaterialTheme.colorScheme.primaryContainer else MaterialTheme.colorScheme.surfaceContainerHigh, label = "choice")
				val fg = (if (on) MaterialTheme.colorScheme.onPrimaryContainer else MaterialTheme.colorScheme.onSurfaceVariant).copy(alpha = if (enabled) 1f else Alpha.DISABLED)
				val name = c.label ?: c.value
				Column(
					Modifier.weight(1f).fillMaxHeight().clip(InnerTileShape).background(bg)
						.selectable(selected = on, enabled = enabled && !on, role = Role.Button) {
							if (action != null && w.param != null) a.launch(action, withParam(w.params, w.param, JsonPrimitive(c.value)), label + PHRASE_SEPARATOR + name)
						}
						.padding(vertical = Space.mPlus, horizontal = Space.xsPlus),
					horizontalAlignment = Alignment.CenterHorizontally,
					verticalArrangement = Arrangement.spacedBy(Space.xsPlus),
				) {
					if (action?.id == PROFILE_SET) Icon(profileIcon(c.value), null, tint = fg)
					Text(name, style = MaterialTheme.typography.labelLarge, color = fg, maxLines = 2, overflow = TextOverflow.Ellipsis)
				}
			}
			// The last row's choices as wide as the others.
			if (choices.size > CHOICES_IN_A_ROW) repeat((CHOICES_IN_A_ROW - choices.size % CHOICES_IN_A_ROW) % CHOICES_IN_A_ROW) { Spacer(Modifier.weight(1f)) }
		}
	}
}

private const val CHOICES_IN_A_ROW = 3
private const val PROFILE_SET = "profile.set"

/** The power profiles' pictures, as 0.5 drew them. */
private fun profileIcon(p: String): ImageVector = when (p) {
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
		val steps = param?.step?.takeIf { it > 0 }?.let { (((max - min) / it).toInt() - 1).coerceIn(0, MAX_STEPS) } ?: 0
		Slider(
			value = shown.toFloat(),
			onValueChange = { dragging = it },
			onValueChangeFinished = {
				val v = dragging
				dragging = null
				if (v != null && action != null && param != null) {
					val snapped = Params.snap(v.toDouble(), param)
					a.launch(action, withParam(w.params, param.id, Params.number(snapped, param)), label + PHRASE_SEPARATOR + Fmt.withUnit(Fmt.round(snapped, if (snapped % 1.0 == 0.0) 0 else 1), unit))
				}
			},
			valueRange = min.toFloat()..max.toFloat(),
			steps = steps,
			enabled = a.enabled(action, s) && param != null,
		)
	}
}

/** Past this many steps a slider is drawn without its ticks. */
private const val MAX_STEPS = 100

/**
 * The values an action needs that the widget leaves open: a choice among its choices, a number on
 * its range's steps, a switch, or typed; run once each takes its value.
 */
@OptIn(ExperimentalLayoutApi::class)
@Composable
fun ParamForm(req: FormRequest, onDismiss: () -> Unit, onDone: (JsonObject) -> Unit) {
	val asked = remember(req) { Params.asked(req.action, req.fixed) }
	val values = remember(req) { mutableStateMapOf<String, JsonElement?>().apply { for (p in asked) put(p.id, Params.initial(p)) } }
	val texts = remember(req) { mutableStateMapOf<String, String>().apply { for (p in asked) put(p.id, values[p.id]?.text().orEmpty()) } }
	val params = buildJsonObject {
		req.fixed.forEach { (k, v) -> put(k, v) }
		for (p in asked) values[p.id]?.takeIf { it !is JsonNull }?.let { put(p.id, it) }
	}
	val ok = Params.complete(req.action, params)
	GlassDialog(
		onDismiss = onDismiss,
		title = req.action.name.ifEmpty { req.phrase },
		icon = icon(req.action.icon),
		text = {
			Column(verticalArrangement = Arrangement.spacedBy(Space.m)) {
				for (p in asked) {
					val name = p.label ?: p.id
					when (p.kind) {
						"choice" -> {
							Text(name, style = MaterialTheme.typography.labelLarge, color = MaterialTheme.colorScheme.primary)
							FlowRow(horizontalArrangement = Arrangement.spacedBy(Space.s)) {
								for (c in p.choices.orEmpty()) {
									FilterChip(selected = values[p.id]?.text() == c.value, onClick = { values[p.id] = JsonPrimitive(c.value) }, label = { Text(c.label ?: c.value) })
								}
							}
						}
						"bool" -> Row(verticalAlignment = Alignment.CenterVertically) {
							Text(name, style = MaterialTheme.typography.bodyLarge, modifier = Modifier.weight(1f))
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
		confirm = { TextButton(onClick = { onDone(params) }, enabled = ok) { Text(stringResource(R.string.control_continue)) } },
		dismiss = { TextButton(onClick = onDismiss) { Text(stringResource(R.string.cancel)) } },
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
		val steps = p.step?.takeIf { it > 0 }?.let { (((max - min) / it).toInt() - 1).coerceIn(0, MAX_STEPS) } ?: 0
		Slider(value = v.toFloat(), onValueChange = { onChange(Params.snap(it.toDouble(), p)) }, valueRange = min.toFloat()..max.toFloat(), steps = steps)
	}
}

/** Control is off on the computer: why, and how to turn it on there. Action widgets stay, disabled. */
@Composable
fun ControlOff(reason: String?) {
	Section(stringResource(R.string.control_off), ElIcons.Lock, accent = MaterialTheme.colorScheme.secondary) {
		when (reason) {
			"held_off" -> {
				Text(stringResource(R.string.control_held_off_body), style = MaterialTheme.typography.bodyMedium)
				CodeBlock("elamond control on")
			}
			"settings_unreadable" -> {
				Text(stringResource(R.string.control_unreadable_body), style = MaterialTheme.typography.bodyMedium)
				CodeBlock("elamond check")
			}
			else -> {
				Text(stringResource(R.string.control_off_body), style = MaterialTheme.typography.bodyMedium)
				CodeBlock("control = true")
				Text(stringResource(R.string.control_off_where), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
			}
		}
	}
}

/** A colour at the alpha of a disabled control. */
fun Color.disabled(enabled: Boolean): Color = if (enabled) this else copy(alpha = Alpha.DISABLED)
