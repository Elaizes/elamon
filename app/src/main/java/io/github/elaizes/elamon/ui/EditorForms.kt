package io.github.elaizes.elamon.ui

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Add
import androidx.compose.material.icons.filled.ArrowDropDown
import androidx.compose.material.icons.filled.Clear
import androidx.compose.material.icons.filled.Delete
import androidx.compose.material.icons.filled.KeyboardArrowDown
import androidx.compose.material.icons.filled.KeyboardArrowUp
import androidx.compose.material.icons.filled.Search
import androidx.compose.material3.FilterChip
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Surface
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.booleanOrNull
import kotlinx.serialization.json.doubleOrNull
import io.github.elaizes.elamon.R
import io.github.elaizes.elamon.data.Drafts
import io.github.elaizes.elamon.data.EditorDescription
import io.github.elaizes.elamon.data.EditorKey
import io.github.elaizes.elamon.data.ProtocolJson
import io.github.elaizes.elamon.data.bool
import io.github.elaizes.elamon.data.isSettingsId
import io.github.elaizes.elamon.data.text

/*
 * The editor's forms, made from the service's description of each key (docs/protocol.md, "The
 * dashboard editor"): one field for each kind. A kind no field knows is JSON text, and so is a key
 * the description doesn't have: kept as it is, never dropped.
 */

/** What a form's fields need besides their own key and value. */
class FormEnv(
	val desc: EditorDescription,
	/** Opens the widget at a place of the draft (a group's child) in the sheet. */
	val openWidget: (List<Int>) -> Unit,
)

/** Choices shown as chips up to so many; more, in a list to pick from. */
private const val CHIPS_MAX = 6

/** An `int` from its `min` to its `max` as chips while there are no more of them than this. */
private const val INT_CHIPS_MAX = 6

/** The most lines a JSON field grows to before it scrolls. */
private const val JSON_LINES = 6

/** The most lines a `text` field (one that may hold values) grows to. */
private const val TEXT_LINES = 4

/** Choices to pick from with a search field from so many. */
private const val SEARCH_FROM = 12

/** The most of a long list a picker shows at once: a search finds the rest. */
private const val PICK_SHOWN = 200

/** A record's border. */
private val OUTLINE = 1.dp

private val NUMBER = Regex("""-?\d+(\.\d+)?([eE][-+]?\d+)?""")

/** A JSON value as a field shows it: a string as it is, anything else as JSON; nothing for none. */
fun asText(v: JsonElement?): String = when {
	v == null || v is JsonNull -> ""
	v is JsonPrimitive && v.isString -> v.content
	else -> v.toString()
}

/** What an `int` field's text sets: the number, or the text as it is for the computer to refuse. */
fun intOf(t: String): JsonElement? {
	val s = t.trim()
	if (s.isEmpty()) return null
	return s.toLongOrNull()?.let(::JsonPrimitive) ?: JsonPrimitive(t)
}

/** What a `number` field's text sets: whole numbers as integers, a decimal comma taken as a point. */
fun numberOf(t: String): JsonElement? {
	val s = t.trim().replace(',', '.')
	if (s.isEmpty()) return null
	if (!NUMBER.matches(s)) return JsonPrimitive(t)
	return s.toLongOrNull()?.let(::JsonPrimitive) ?: s.toDoubleOrNull()?.let(::JsonPrimitive) ?: JsonPrimitive(t)
}

/** What a `scalar` field's text sets: true, false or a number as such, anything else as a string. */
fun scalarOf(t: String): JsonElement? = when {
	t.isEmpty() -> null
	t == "true" -> JsonPrimitive(true)
	t == "false" -> JsonPrimitive(false)
	NUMBER.matches(t) -> numberOf(t)
	else -> JsonPrimitive(t)
}

/** JSON text read as JSON; text that isn't, kept as a string (the computer says what's wrong with it). Null for none. */
fun jsonOf(t: String): JsonElement? {
	if (t.isBlank()) return null
	return runCatching { ProtocolJson.parseToJsonElement(t) }.getOrElse { JsonPrimitive(t) }
}

/** Whether [t] is JSON. */
private fun isJson(t: String): Boolean = t.isBlank() || runCatching { ProtocolJson.parseToJsonElement(t) }.isSuccess

/** An empty item of a list or a map of [key]s, for the field to fill. */
fun emptyOf(key: EditorKey?): JsonElement = when (key?.kind) {
	"record", "map" -> JsonObject(emptyMap())
	"list", "widgets" -> JsonArray(emptyList())
	"bool" -> JsonPrimitive(false)
	"int", "number" -> JsonPrimitive(0)
	else -> key?.default ?: JsonPrimitive("")
}

/** Whether [v] is what a key of [kind] holds: which of an `any`'s keys it is. */
fun holds(key: EditorKey, v: JsonElement, desc: EditorDescription): Boolean {
	val p = v as? JsonPrimitive
	val string = p != null && p.isString
	return when (key.kind) {
		"number", "int" -> p != null && !p.isString && p.doubleOrNull != null
		"bool" -> p != null && !p.isString && p.booleanOrNull != null
		"list", "widgets" -> v is JsonArray
		"map", "record" -> v is JsonObject
		"address" -> string && desc.values.any { it.address == p.content }
		"ref" -> string && refItems(key.to, desc)?.any { it.value == p.content } != false
		"enum", "format" -> string && (key.kind == "format" || p.content in key.choiceValues)
		"string", "text", "id" -> string
		"scalar" -> p != null && p !is JsonNull
		else -> false
	}
}

/** One thing to pick: its [value], as it is written, and what shows of it. */
data class PickItem(val value: String, val title: String, val subtitle: String? = null)

/** What a `ref` to [to] may name, as the description has them; null for something no picker knows. */
fun refItems(to: String?, desc: EditorDescription): List<PickItem>? = when (to) {
	"action" -> desc.actions.map { PickItem(it.id, it.name.ifBlank { it.id }, listOf(it.id, it.kind).filter(String::isNotBlank).joinToString(" · ")) }
	"dashboard" -> desc.dashboards.map { PickItem(it.id, it.title.ifBlank { it.id }, it.id) }
	"collection" -> desc.collections.map { PickItem(it.name, it.name, it.fields.joinToString(", ").ifEmpty { null }) }
	"link" -> desc.links.map { PickItem(it.id, it.name.ifBlank { it.id }, it.id) }
	"source" -> desc.sources.map { PickItem(it.id, it.id, it.kind.ifBlank { null }) }
	else -> null
}

/** What an `any`'s chip calls one of its kinds; a kind the app has no word for, by its name. */
@Composable
private fun kindWord(key: EditorKey): String {
	val res: Int? = when (key.kind) {
		"number", "scalar" -> R.string.editor_kind_number
		"int" -> R.string.editor_kind_int
		"bool" -> R.string.editor_kind_bool
		"string", "text", "id" -> R.string.editor_kind_text
		"address" -> R.string.editor_kind_address
		"enum", "format" -> R.string.editor_kind_choice
		"list" -> R.string.editor_kind_list
		"map", "record" -> R.string.editor_kind_table
		"widgets" -> R.string.editor_kind_widgets
		"ref" -> when (key.to) {
			"action" -> R.string.editor_kind_action
			"dashboard" -> R.string.editor_kind_dashboard
			"collection" -> R.string.editor_kind_collection
			"link" -> R.string.editor_kind_link
			"source" -> R.string.editor_kind_source
			else -> null
		}
		else -> null
	}
	return res?.let { stringResource(it) } ?: key.kind
}

/**
 * One key: its name (marked when it must be there), its field by its kind, its hint, and what the
 * computer found wrong with it. With [fixedIn], a lower settings file sets it: it can be changed,
 * not taken out. [path] is the place of the widget it belongs to, for a group's children.
 */
@Composable
fun KeyField(
	key: EditorKey,
	value: JsonElement?,
	onChange: (JsonElement?) -> Unit,
	env: FormEnv,
	mistakes: List<String> = emptyList(),
	fixedIn: String? = null,
	path: List<Int> = emptyList(),
) {
	val name = if (key.required) stringResource(R.string.editor_required, key.name) else key.name
	Column(verticalArrangement = Arrangement.spacedBy(Space.xs)) {
		Row(verticalAlignment = Alignment.CenterVertically) {
			Text(name, style = MaterialTheme.typography.labelLarge, color = MaterialTheme.colorScheme.primary, modifier = Modifier.weight(1f))
			if (key.kind == "bool") {
				Switch(checked = (value ?: key.default).bool() == true, onCheckedChange = { onChange(JsonPrimitive(it)) })
			}
			if (value != null && fixedIn == null) {
				IconButton(onClick = { onChange(null) }) {
					Icon(Icons.Default.Clear, stringResource(R.string.editor_clear, key.name), Modifier.size(Sizes.iconM))
				}
			}
		}
		if (key.kind != "bool") KeyControl(key, value, onChange, env, path, isError = mistakes.isNotEmpty())
		key.hint?.takeIf { it.isNotBlank() }?.let { Hint(it) }
		if (value == null && key.default != null && key.default !is JsonNull) Hint(stringResource(R.string.editor_default, asText(key.default)))
		fixedIn?.let { Hint(stringResource(R.string.editor_fixed, it)) }
		Mistakes(mistakes)
	}
}

@Composable
private fun Hint(text: String) {
	Text(text, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
}

/** What the computer found wrong, each a line in the error's colour. */
@Composable
fun Mistakes(mistakes: List<String>) {
	for (m in mistakes) Text(m, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.error)
}

/** A key's field alone, without its name: what a list's or a map's item is edited with too. */
@Composable
fun KeyControl(key: EditorKey, value: JsonElement?, onChange: (JsonElement?) -> Unit, env: FormEnv, path: List<Int> = emptyList(), isError: Boolean = false) {
	when (key.kind) {
		"string", "text" -> TextControl(value, onChange, key, isError, lines = if (key.kind == "text") TEXT_LINES else 1)
		"id" -> TextControl(value, onChange, key, isError || asText(value).let { it.isNotEmpty() && !isSettingsId(it) }, keyboard = KeyboardType.Ascii)
		"int" -> IntControl(key, value, onChange, isError)
		"number" -> TextControl(value, onChange, key, isError, keyboard = KeyboardType.Decimal, parse = ::numberOf)
		"scalar" -> TextControl(value, onChange, key, isError, parse = ::scalarOf)
		"bool" -> Switch(checked = (value ?: key.default).bool() == true, onCheckedChange = { onChange(JsonPrimitive(it)) })
		"address" -> AddressControl(key, value, onChange, env.desc, isError)
		"ref" -> RefControl(key, value, onChange, env.desc, isError)
		"enum" -> EnumControl(key, value, onChange, isError)
		"format" -> FormatControl(key, value, onChange, isError)
		"list" -> ListControl(key, value, onChange, env, path)
		"map" -> MapControl(key, value, onChange, env, path)
		"record" -> RecordControl(key, value, onChange, env, path)
		"widgets" -> WidgetsControl(value, onChange, env, path)
		"any" -> AnyControl(key, value, onChange, env, path, isError)
		else -> JsonControl(value, onChange, isError)
	}
}

/**
 * A text field over a JSON value: its own text while it is typed in (an unfinished number isn't
 * rewritten under the finger), what it sets by [parse], and the text again when the value changes
 * from elsewhere (an undo).
 */
@Composable
private fun TextControl(
	value: JsonElement?,
	onChange: (JsonElement?) -> Unit,
	key: EditorKey?,
	isError: Boolean,
	lines: Int = 1,
	keyboard: KeyboardType = KeyboardType.Text,
	parse: (String) -> JsonElement? = { t -> t.ifEmpty { null }?.let(::JsonPrimitive) },
	trailing: (@Composable () -> Unit)? = null,
) {
	var text by remember { mutableStateOf(asText(value)) }
	LaunchedEffect(value) { if (parse(text) != value) text = asText(value) }
	OutlinedTextField(
		value = text,
		onValueChange = {
			text = it
			onChange(parse(it))
		},
		placeholder = key?.default?.takeIf { it !is JsonNull }?.let { d -> { Text(asText(d)) } },
		isError = isError,
		singleLine = lines == 1,
		maxLines = lines,
		trailingIcon = trailing,
		keyboardOptions = KeyboardOptions(
			keyboardType = keyboard,
			// Ids, addresses and JSON are typed as the computer reads them.
			capitalization = if (keyboard == KeyboardType.Text) KeyboardCapitalization.Sentences else KeyboardCapitalization.None,
			autoCorrectEnabled = keyboard == KeyboardType.Text,
		),
		modifier = Modifier.fillMaxWidth(),
	)
}

/** A whole number: chips from its `min` to its `max` while they are few (a widget's width), else typed in. */
@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun IntControl(key: EditorKey, value: JsonElement?, onChange: (JsonElement?) -> Unit, isError: Boolean) {
	val min = key.min?.toLong()
	val max = key.max?.toLong()
	if (min != null && max != null && max >= min && max - min < INT_CHIPS_MAX) {
		val now = (value as? JsonPrimitive)?.content?.toLongOrNull() ?: (key.default as? JsonPrimitive)?.content?.toLongOrNull()
		FlowRow(horizontalArrangement = Arrangement.spacedBy(Space.s)) {
			for (n in min..max) FilterChip(selected = now == n, onClick = { onChange(JsonPrimitive(n)) }, label = { Text(n.toString()) })
		}
		return
	}
	val outside = (value as? JsonPrimitive)?.content?.toLongOrNull()?.let { n -> (min != null && n < min) || (max != null && n > max) } == true
	TextControl(value, onChange, key, isError || outside || (value != null && (value as? JsonPrimitive)?.isString == true), keyboard = KeyboardType.Number, parse = ::intOf)
	if (min != null || max != null) Hint(stringResource(R.string.editor_between, min?.toString() ?: "…", max?.toString() ?: "…"))
}

/** A value's address: typed, or picked among those the computer has, each with its reading now. */
@Composable
private fun AddressControl(key: EditorKey, value: JsonElement?, onChange: (JsonElement?) -> Unit, desc: EditorDescription, isError: Boolean) {
	var picking by rememberSaveable { mutableStateOf(false) }
	TextControl(
		value, onChange, key, isError, keyboard = KeyboardType.Ascii,
		trailing = { IconButton(onClick = { picking = true }) { Icon(Icons.Default.Search, stringResource(R.string.editor_pick_value)) } },
	)
	if (picking) {
		PickDialog(
			title = key.name.ifEmpty { stringResource(R.string.editor_pick_value) },
			items = desc.values.map { PickItem(it.address, it.address, it.now) },
			onPick = { onChange(JsonPrimitive(it)) },
			onDismiss = { picking = false },
		)
	}
}

/** The id of something the computer has: an action, a dashboard, a collection, a link or a source. */
@Composable
private fun RefControl(key: EditorKey, value: JsonElement?, onChange: (JsonElement?) -> Unit, desc: EditorDescription, isError: Boolean) {
	val items = refItems(key.to, desc)
	if (items == null) {
		TextControl(value, onChange, key, isError, keyboard = KeyboardType.Ascii)
		return
	}
	val now = value.text()
	val item = items.find { it.value == now }
	var picking by rememberSaveable { mutableStateOf(false) }
	PickerField(item?.let { listOfNotNull(it.title, it.subtitle?.takeIf { s -> s != it.title }).joinToString(" · ") } ?: now.orEmpty(), isError) { picking = true }
	if (picking) {
		PickDialog(key.name, items, onPick = { onChange(JsonPrimitive(it)) }, onDismiss = { picking = false })
	}
}

/** One of its `choices`: chips while they are few, else picked from a list. */
@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun EnumControl(key: EditorKey, value: JsonElement?, onChange: (JsonElement?) -> Unit, isError: Boolean) {
	val choices = key.choiceValues
	val now = value.text() ?: key.default.text()
	if (choices.size <= CHIPS_MAX) {
		FlowRow(horizontalArrangement = Arrangement.spacedBy(Space.s)) {
			for (c in choices) FilterChip(selected = now == c, onClick = { onChange(JsonPrimitive(c)) }, label = { Text(c) })
			// One the description doesn't list (from a newer service's settings) stays, shown as it is.
			if (value != null && value.text() !in choices) FilterChip(selected = true, onClick = {}, label = { Text(asText(value)) })
		}
		return
	}
	var picking by rememberSaveable { mutableStateOf(false) }
	PickerField(value?.let(::asText).orEmpty(), isError, placeholder = key.default.text()) { picking = true }
	if (picking) PickDialog(key.name, choices.map { PickItem(it, it) }, onPick = { onChange(JsonPrimitive(it)) }, onDismiss = { picking = false })
}

/** One of its `choices`, or a pattern of its own (`{:.1}`): typed, or picked. */
@Composable
private fun FormatControl(key: EditorKey, value: JsonElement?, onChange: (JsonElement?) -> Unit, isError: Boolean) {
	var picking by rememberSaveable { mutableStateOf(false) }
	TextControl(
		value, onChange, key, isError, keyboard = KeyboardType.Ascii,
		trailing = { IconButton(onClick = { picking = true }) { Icon(Icons.Default.ArrowDropDown, stringResource(R.string.editor_pick)) } },
	)
	if (picking) PickDialog(key.name, key.choiceValues.map { PickItem(it, it) }, onPick = { onChange(JsonPrimitive(it)) }, onDismiss = { picking = false })
}

/** Items of the kind its `of` says, each its own field, moved and taken out by the buttons beside it. */
@Composable
private fun ListControl(key: EditorKey, value: JsonElement?, onChange: (JsonElement?) -> Unit, env: FormEnv, path: List<Int>) {
	val of = key.item
	val items = (value as? JsonArray)?.toList()
	if (value != null && items == null) {
		// A single value where a list goes: as the computer may take it, kept and shown as JSON.
		JsonControl(value, onChange, isError = false)
		return
	}
	val list = items.orEmpty()
	Column(verticalArrangement = Arrangement.spacedBy(Space.s)) {
		list.forEachIndexed { i, item ->
			ItemRow(
				onUp = if (i > 0) ({ onChange(JsonArray(list.toMutableList().apply { add(i - 1, removeAt(i)) })) }) else null,
				onDown = if (i < list.lastIndex) ({ onChange(JsonArray(list.toMutableList().apply { add(i + 1, removeAt(i)) })) }) else null,
				onRemove = { onChange(JsonArray(list.filterIndexed { j, _ -> j != i }).takeIf { it.isNotEmpty() }) },
			) {
				val itemKey = of ?: EditorKey()
				KeyControl(itemKey, item, { v -> onChange(JsonArray(list.toMutableList().apply { set(i, v ?: emptyOf(of)) })) }, env, path)
			}
		}
		TextButton(onClick = { onChange(JsonArray(list + emptyOf(of))) }) {
			Icon(Icons.Default.Add, null, Modifier.size(Sizes.iconM))
			Text(stringResource(R.string.editor_add_item), Modifier.padding(start = Space.xs))
		}
	}
}

/** Keys of its own naming, each with a value of the kind its `of` says. */
@Composable
private fun MapControl(key: EditorKey, value: JsonElement?, onChange: (JsonElement?) -> Unit, env: FormEnv, path: List<Int>) {
	val of = key.item
	val map = value as? JsonObject
	if (value != null && map == null) {
		JsonControl(value, onChange, isError = false)
		return
	}
	val entries = map.orEmpty().entries.toList()
	var adding by rememberSaveable { mutableStateOf<String?>(null) }
	Column(verticalArrangement = Arrangement.spacedBy(Space.s)) {
		for ((k, v) in entries) {
			ItemRow(onRemove = { onChange(JsonObject(map.orEmpty() - k).takeIf { it.isNotEmpty() }) }) {
				Column(verticalArrangement = Arrangement.spacedBy(Space.xs)) {
					Text(k, style = MaterialTheme.typography.labelMedium, fontFamily = FontFamily.Monospace, maxLines = 1, overflow = TextOverflow.Ellipsis)
					KeyControl(of ?: EditorKey(), v, { nv -> onChange(Drafts.withKey(JsonObject(map.orEmpty()), k, nv ?: emptyOf(of))) }, env, path)
				}
			}
		}
		val pending = adding
		if (pending == null) {
			TextButton(onClick = { adding = "" }) {
				Icon(Icons.Default.Add, null, Modifier.size(Sizes.iconM))
				Text(stringResource(R.string.editor_add_item), Modifier.padding(start = Space.xs))
			}
		} else {
			Row(verticalAlignment = Alignment.CenterVertically) {
				OutlinedTextField(
					value = pending,
					onValueChange = { adding = it },
					label = { Text(stringResource(R.string.editor_item_key)) },
					singleLine = true,
					isError = pending in map.orEmpty(),
					keyboardOptions = KeyboardOptions(capitalization = KeyboardCapitalization.None, autoCorrectEnabled = false),
					modifier = Modifier.weight(1f),
				)
				TextButton(
					enabled = pending.isNotBlank() && pending !in map.orEmpty(),
					onClick = {
						onChange(Drafts.withKey(JsonObject(map.orEmpty()), pending.trim(), emptyOf(of)))
						adding = null
					},
				) { Text(stringResource(R.string.editor_add_item)) }
				IconButton(onClick = { adding = null }) { Icon(Icons.Default.Clear, stringResource(R.string.cancel)) }
			}
		}
	}
}

/** A table of the keys its `record` names (a column, a bar), each its own field; keys the description hasn't, as JSON. */
@Composable
private fun RecordControl(key: EditorKey, value: JsonElement?, onChange: (JsonElement?) -> Unit, env: FormEnv, path: List<Int>) {
	val keys = key.record?.let { env.desc.records[it] }
	val obj = value as? JsonObject
	if (keys == null || (value != null && obj == null)) {
		JsonControl(value, onChange, isError = false)
		return
	}
	val o = obj.orEmpty().let(::JsonObject)
	Surface(shape = RoundedCornerShape(Radius.m), border = BorderStroke(OUTLINE, MaterialTheme.colorScheme.outlineVariant), color = MaterialTheme.colorScheme.surfaceContainerLow) {
		Column(Modifier.padding(Space.m), verticalArrangement = Arrangement.spacedBy(Space.m)) {
			ObjectFields(keys, o, { k, v -> onChange(Drafts.withKey(o, k, v).takeIf { m -> m.isNotEmpty() }) }, env, path)
		}
	}
}

/** The fields of [o] by [keys], then those of its keys the description hasn't, as JSON. */
@Composable
fun ObjectFields(
	keys: List<EditorKey>,
	o: JsonObject,
	/** A key set to a value, or taken out with null. */
	onKey: (String, JsonElement?) -> Unit,
	env: FormEnv,
	path: List<Int>,
	mistakes: Map<String, List<String>> = emptyMap(),
	fixed: (String) -> String? = { null },
	skip: Set<String> = emptySet(),
) {
	for (k in keys) {
		if (k.key in skip) continue
		KeyField(k, o[k.key], { v -> onKey(k.key, v) }, env, mistakes[k.key].orEmpty(), fixed(k.key), path)
	}
	for (name in Drafts.unknownKeys(o, keys)) {
		if (name in skip) continue
		val fixedIn = fixed(name)
		KeyField(EditorKey(key = name, kind = ""), o[name], { v -> onKey(name, v) }, env, mistakes[name].orEmpty(), fixedIn, path)
	}
}

/** A group's widgets: each by its type and label, opened in the sheet with a tap; added from the types there are. */
@Composable
private fun WidgetsControl(value: JsonElement?, onChange: (JsonElement?) -> Unit, env: FormEnv, path: List<Int>) {
	val list = (value as? JsonArray)?.toList().orEmpty()
	var adding by rememberSaveable { mutableStateOf(false) }
	Column(verticalArrangement = Arrangement.spacedBy(Space.xs)) {
		list.forEachIndexed { i, w ->
			val o = w as? JsonObject
			ItemRow(
				onUp = if (i > 0) ({ onChange(JsonArray(list.toMutableList().apply { add(i - 1, removeAt(i)) })) }) else null,
				onDown = if (i < list.lastIndex) ({ onChange(JsonArray(list.toMutableList().apply { add(i + 1, removeAt(i)) })) }) else null,
				onRemove = { onChange(JsonArray(list.filterIndexed { j, _ -> j != i })) },
			) {
				val title = o?.let(env.desc::titleOf) ?: asText(w)
				Text(
					title,
					style = MaterialTheme.typography.bodyLarge,
					maxLines = 2,
					overflow = TextOverflow.Ellipsis,
					modifier = Modifier
						.fillMaxWidth()
						.heightIn(min = Sizes.touchTarget)
						.clickable(enabled = o != null, role = Role.Button) { env.openWidget(path + i) }
						.padding(vertical = Space.m),
				)
			}
		}
		TextButton(onClick = { adding = true }) {
			Icon(Icons.Default.Add, null, Modifier.size(Sizes.iconM))
			Text(stringResource(R.string.editor_add), Modifier.padding(start = Space.xs))
		}
	}
	if (adding) {
		AddWidgetDialog(env.desc, onDismiss = { adding = false }) { onChange(JsonArray(list + it)) }
	}
}

/** One of the kinds its `of` names: chips to choose which, then that kind's field. */
@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun AnyControl(key: EditorKey, value: JsonElement?, onChange: (JsonElement?) -> Unit, env: FormEnv, path: List<Int>, isError: Boolean) {
	val alts = key.alternatives
	if (alts.isEmpty()) {
		JsonControl(value, onChange, isError)
		return
	}
	val held = value?.let { v -> alts.indexOfFirst { holds(it, v, env.desc) }.takeIf { it >= 0 } }
		// A string is a value's address, or any text: the first alternative that takes text.
		?: value?.let { v -> alts.indexOfFirst { (v as? JsonPrimitive)?.isString == true && it.kind in TEXT_KINDS }.takeIf { it >= 0 } }
	var chosen by rememberSaveable { mutableIntStateOf(held ?: 0) }
	LaunchedEffect(held) { if (held != null) chosen = held }
	Column(verticalArrangement = Arrangement.spacedBy(Space.xs)) {
		if (alts.size > 1) {
			FlowRow(horizontalArrangement = Arrangement.spacedBy(Space.s)) {
				alts.forEachIndexed { i, alt -> FilterChip(selected = chosen == i, onClick = { chosen = i }, label = { Text(kindWord(alt)) }) }
			}
		}
		val alt = alts[chosen.coerceIn(alts.indices)]
		// A value of another kind than the one chosen is replaced by what is typed or picked.
		KeyControl(alt, value?.takeIf { holds(alt, it, env.desc) || chosen == held }, onChange, env, path, isError)
	}
}

private val TEXT_KINDS = setOf("address", "string", "text", "ref", "format", "enum", "id")

/** Any JSON: what the app has no field for, kept as it is. Text that isn't JSON is kept as a string, and said so. */
@Composable
fun JsonControl(value: JsonElement?, onChange: (JsonElement?) -> Unit, isError: Boolean) {
	// JSON text: a string with its quotes.
	var text by remember { mutableStateOf(value?.toString().orEmpty()) }
	LaunchedEffect(value) { if (jsonOf(text) != value) text = value?.toString().orEmpty() }
	val json = isJson(text)
	OutlinedTextField(
		value = text,
		onValueChange = {
			text = it
			onChange(jsonOf(it))
		},
		isError = isError || !json,
		maxLines = JSON_LINES,
		textStyle = MaterialTheme.typography.bodyMedium.copy(fontFamily = FontFamily.Monospace),
		keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Ascii, capitalization = KeyboardCapitalization.None, autoCorrectEnabled = false),
		modifier = Modifier.fillMaxWidth(),
	)
	if (!json) Mistakes(listOf(stringResource(R.string.editor_not_json)))
}

/** An item of a list or a map, with its buttons: up, down, out. */
@Composable
private fun ItemRow(onUp: (() -> Unit)? = null, onDown: (() -> Unit)? = null, onRemove: () -> Unit, content: @Composable () -> Unit) {
	Row(verticalAlignment = Alignment.CenterVertically) {
		Box(Modifier.weight(1f)) { content() }
		if (onUp != null || onDown != null) {
			Column {
				IconButton(onClick = { onUp?.invoke() }, enabled = onUp != null) { Icon(Icons.Default.KeyboardArrowUp, stringResource(R.string.editor_move_earlier)) }
				IconButton(onClick = { onDown?.invoke() }, enabled = onDown != null) { Icon(Icons.Default.KeyboardArrowDown, stringResource(R.string.editor_move_later)) }
			}
		}
		IconButton(onClick = onRemove) { Icon(Icons.Default.Delete, stringResource(R.string.editor_remove_item), tint = MaterialTheme.colorScheme.onSurfaceVariant) }
	}
}

/** A field that is picked rather than typed: what is chosen, and a tap to choose. */
@Composable
fun PickerField(text: String, isError: Boolean = false, placeholder: String? = null, onClick: () -> Unit) {
	Box {
		OutlinedTextField(
			value = text,
			onValueChange = {},
			readOnly = true,
			isError = isError,
			singleLine = true,
			placeholder = placeholder?.let { p -> { Text(p) } },
			trailingIcon = { Icon(Icons.Default.ArrowDropDown, null) },
			modifier = Modifier.fillMaxWidth(),
		)
		// The whole field opens the list; the text field itself takes no typing.
		Box(Modifier.matchParentSize().clickable(role = Role.Button, onClickLabel = stringResource(R.string.editor_pick), onClick = onClick))
	}
}

/** A list to pick one of [items] from, searched where it is long. */
@Composable
fun PickDialog(title: String, items: List<PickItem>, onPick: (String) -> Unit, onDismiss: () -> Unit) {
	var query by rememberSaveable { mutableStateOf("") }
	val q = query.trim()
	val found = if (q.isEmpty()) items else items.filter { i -> listOfNotNull(i.value, i.title, i.subtitle).any { it.contains(q, ignoreCase = true) } }
	GlassDialog(
		onDismiss = onDismiss,
		title = title,
		text = {
			Column(verticalArrangement = Arrangement.spacedBy(Space.xs)) {
				if (items.size >= SEARCH_FROM) {
					OutlinedTextField(
						value = query,
						onValueChange = { query = it },
						label = { Text(stringResource(R.string.editor_search)) },
						singleLine = true,
						leadingIcon = { Icon(Icons.Default.Search, null) },
						keyboardOptions = KeyboardOptions(capitalization = KeyboardCapitalization.None, autoCorrectEnabled = false),
						modifier = Modifier.fillMaxWidth(),
					)
				}
				if (found.isEmpty()) Hint(stringResource(R.string.editor_nothing_found))
				for (i in found.take(PICK_SHOWN)) {
					Column(
						Modifier
							.fillMaxWidth()
							.heightIn(min = Sizes.touchTarget)
							.clickable(role = Role.Button) {
								onPick(i.value)
								onDismiss()
							}
							.padding(vertical = Space.s, horizontal = Space.xs),
					) {
						Text(i.title, style = MaterialTheme.typography.bodyLarge)
						i.subtitle?.takeIf { it.isNotBlank() && it != i.title }?.let { Hint(it) }
					}
				}
				if (found.size > PICK_SHOWN) Hint(pluralStringResource(R.plurals.editor_search_more, found.size - PICK_SHOWN, found.size - PICK_SHOWN))
			}
		},
		confirm = { TextButton(onClick = onDismiss) { Text(stringResource(R.string.cancel)) } },
	)
}

/** The widget types the computer offers, each by its label with its example: the one picked is added as its example has it. */
@Composable
fun AddWidgetDialog(desc: EditorDescription, onDismiss: () -> Unit, onAdd: (JsonObject) -> Unit) {
	PickDialog(
		title = stringResource(R.string.editor_add_title),
		items = desc.types.map { t -> PickItem(t.type, t.name, t.example?.let(::exampleLine)) },
		onPick = { type -> desc.type(type)?.let { onAdd(it.newWidget()) } },
		onDismiss = onDismiss,
	)
}

/** An example widget in a line: its keys and values, the type left out (it is the title). */
fun exampleLine(example: JsonObject): String =
	example.filterKeys { it != "type" }.entries.joinToString(" · ") { (k, v) -> "$k: " + asText(v) }
