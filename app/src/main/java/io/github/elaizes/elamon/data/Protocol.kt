@file:OptIn(ExperimentalSerializationApi::class)

package io.github.elaizes.elamon.data

import kotlinx.serialization.EncodeDefault
import kotlinx.serialization.ExperimentalSerializationApi
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.booleanOrNull
import kotlinx.serialization.json.doubleOrNull
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive

/**
 * What elamond sends, API 2 (docs/protocol.md). Unknown fields are ignored so newer versions still
 * work. Ids and times have no default: a message without them is broken, not zero.
 */
val ProtocolJson = Json {
	ignoreUnknownKeys = true
	classDiscriminator = "type"
	explicitNulls = false
	coerceInputValues = true
}

/** The API this app speaks: a service with any other is asked to be updated (or the app). */
const val APP_API = 2

/** What the app takes the service's tick and history to be while it doesn't say. */
const val DEFAULT_INTERVAL_MS = 1000
const val DEFAULT_HISTORY_SECONDS = 600

@Serializable
data class Hello(
	val app: String,
	val api: Int,
	val id: String,
	val name: String,
	val version: String,
	/** The oldest app version the service works with. */
	@SerialName("min_app") val minApp: String? = null,
	/** The version of the dictionary of widget types and their keys. */
	val widgets: Int? = null,
	/** What this computer can offer at all, whatever a client's role. */
	val features: List<String> = emptyList(),
)

/** `POST /api/v1/rekey`: a key of this phone's own, named [device] in the computer's list of phones. */
@Serializable
data class RekeyRequest(val device: String)

/** What pairing answers, and asking for a key of this phone's own (`POST /api/v1/rekey`). */
@Serializable
data class PairResponse(
	val token: String,
	@SerialName("client_id") val clientId: String = "",
	val id: String = "",
	val name: String = "",
	/** What the phone may do there, "glance" to "admin"; null when the service doesn't say. */
	val role: String? = null,
	/** The profile the phone looks and behaves by there; null when the service doesn't say. */
	val profile: String? = null,
)

// ---- GET /api/v1/ui ---------------------------------------------------------------------------

/** What the client shows: its profile, its tabs, the dashboards and the actions, as far as its role goes. */
@Serializable
data class UiDoc(
	/** Changes whenever anything else in the document would. */
	val version: String,
	val service: ServiceInfo,
	val role: String? = null,
	val control: ControlState = ControlState(),
	val profile: Profile = Profile(),
	/** Dashboard ids, in order. */
	val tabs: List<String> = emptyList(),
	/**
	 * The dashboard whose widgets are the small pills under the computer's name, on every tab and
	 * page (the system, the uptime, the power); null for none. Every data line has their states.
	 */
	val header: String? = null,
	/** The tabs, then the dashboards their widgets open. */
	val dashboards: List<Dashboard> = emptyList(),
	val actions: List<Action> = emptyList(),
) {
	fun dashboard(id: String): Dashboard? = dashboards.find { it.id == id }
	fun action(id: String?): Action? = id?.let { a -> actions.find { it.id == a } }
}

@Serializable
data class ServiceInfo(
	val id: String,
	val name: String = "",
	val version: String = "",
	/** The service's own tick: no stream comes faster. */
	@SerialName("interval_ms") val intervalMs: Int = DEFAULT_INTERVAL_MS,
	/** How far back history reaches. */
	@SerialName("history_seconds") val historySeconds: Int = DEFAULT_HISTORY_SECONDS,
)

@Serializable
data class ControlState(
	val enabled: Boolean = false,
	/** Null while control is on, else "settings", "held_off" or "settings_unreadable". */
	@SerialName("off_reason") val offReason: String? = null,
)

/** How the client looks and behaves, as the computer's settings say: the common part and the Android one. */
@Serializable
data class Profile(
	val id: String = "default",
	val name: String = "",
	val theme: String = "system",
	@SerialName("pure_black") val pureBlack: Boolean = false,
	val animations: Boolean = true,
	@SerialName("interval_ms") val intervalMs: Int = DEFAULT_INTERVAL_MS,
	val fahrenheit: Boolean = false,
	@SerialName("network_bits") val networkBits: Boolean = false,
	@SerialName("confirm_actions") val confirmActions: Boolean = true,
	val unlock: UnlockProfile = UnlockProfile(),
	val android: AndroidProfile = AndroidProfile(),
)

@Serializable
data class UnlockProfile(
	@SerialName("for") val kinds: List<String> = listOf("power", "signal", "actions", "units"),
	val grace: String = "every_time",
)

@Serializable
data class AndroidProfile(
	@SerialName("dynamic_color") val dynamicColor: Boolean = true,
	val blur: Boolean = true,
	val haptics: Boolean = true,
	@SerialName("keep_screen_on") val keepScreenOn: Boolean = false,
)

@Serializable
data class Dashboard(
	val id: String,
	val title: String = "",
	val icon: String? = null,
	val widgets: List<Widget> = emptyList(),
)

/**
 * A widget as the service describes it, resolved: every type's keys in one shape, those a type
 * doesn't have left out. A type this app doesn't know is drawn as a notice.
 */
@Serializable
data class Widget(
	val id: String,
	val type: String,
	val span: Int = 2,
	val label: String? = null,
	val icon: String? = null,
	val color: String? = null,
	/** A dashboard tapping it opens. */
	val open: String? = null,
	/** `true` when it holds values (its text comes in the state), else the text itself. */
	val caption: JsonPrimitive? = null,
	/** A group or table that folds, and whether it starts folded; null for one that doesn't. */
	val collapsed: Boolean? = null,
	// value, gauge, meter, status, bars
	val unit: String? = null,
	val format: String? = null,
	val min: Double? = null,
	val max: Double? = null,
	val spark: Boolean? = null,
	// graph
	val series: List<Series>? = null,
	/** Seconds back the graph reaches. */
	val range: Int? = null,
	/** "small", "medium" or "large". */
	val height: String? = null,
	val floor: Double? = null,
	// list
	val items: List<ListItem>? = null,
	// table
	val source: String? = null,
	val columns: List<Column>? = null,
	val sort: Sort? = null,
	val sorts: List<String>? = null,
	val search: Boolean? = null,
	@SerialName("hide_empty") val hideEmpty: Boolean? = null,
	@SerialName("row_open") val rowOpen: String? = null,
	@SerialName("row_actions") val rowActions: List<RowAction>? = null,
	val expand: List<Field>? = null,
	// button, toggle, select, slider
	val action: String? = null,
	/** A button's fixed values for the action's parameters. */
	val params: JsonObject? = null,
	/** The parameter a toggle, select or slider sets. */
	val param: String? = null,
	val on: JsonElement? = null,
	val off: JsonElement? = null,
	// text
	val text: String? = null,
	val level: String? = null,
	// group
	val widgets: List<Widget>? = null,
) {
	/** The caption's text when it holds no values; when it does, the state's. */
	val captionText: String? get() = caption?.takeIf { it.isString }?.content?.ifEmpty { null }
}

@Serializable
data class Series(val label: String? = null, val color: String? = null, val unit: String? = null, val format: String? = null)

@Serializable
data class ListItem(val label: String? = null, val unit: String? = null, val format: String? = null)

@Serializable
data class Column(
	val field: String,
	val label: String? = null,
	val format: String? = null,
	val unit: String? = null,
	val bar: Bar? = null,
	val grow: Boolean = false,
)

/** A cell drawn as a bar, full at [max]: a number, a field of the row, or "top" (a quarter above the column's largest). */
@Serializable
data class Bar(val max: JsonPrimitive? = null)

@Serializable
data class Sort(val field: String, val desc: Boolean = false)

@Serializable
data class RowAction(val action: String, val label: String? = null, val params: JsonObject = JsonObject(emptyMap()))

@Serializable
data class Field(val field: String, val label: String? = null, val format: String? = null, val unit: String? = null)

/** An action of the computer's this client may run; [confirm] and [unlock] are final. */
@Serializable
data class Action(
	val id: String,
	val name: String = "",
	/** "power", "signal", "actions", "units", "profile" or "media". */
	val kind: String = "actions",
	val icon: String? = null,
	val confirm: Boolean = true,
	val unlock: Boolean = false,
	val params: List<Param> = emptyList(),
)

/** One of an action's parameters, and what it takes. */
@Serializable
data class Param(
	val id: String,
	/** "choice", "range", "bool", "integer", "number" or "string". */
	val kind: String,
	val label: String? = null,
	val choices: List<Choice>? = null,
	val min: Double? = null,
	val max: Double? = null,
	val step: Double? = null,
	val unit: String? = null,
	val default: JsonElement? = null,
	/** May be left out without a default. */
	val optional: Boolean = false,
)

@Serializable
data class Choice(val value: String, val label: String? = null)

// ---- GET /api/v1/stream -------------------------------------------------------------------------

@Serializable
sealed interface StreamLine

/** The stream's first line: its id, for changing what it sends, and the version of `ui` now. */
@Serializable
@SerialName("open")
data class StreamOpen(val stream: String, val ui: String) : StreamLine

/** The state of each widget of [dashboard] at [t]; the first after opening or a change also has [history]. */
@Serializable
@SerialName("data")
data class DataLine(
	/** Unix milliseconds on the service's clock. */
	val t: Long,
	val dashboard: String,
	val widgets: Map<String, WidgetState> = emptyMap(),
	val history: Map<String, HistoryEntry>? = null,
) : StreamLine

/** The client's `ui` changed: fetch it again when [ui] isn't the version held. */
@Serializable
@SerialName("ui")
data class UiChanged(val ui: String) : StreamLine

/**
 * A widget's state, every type's keys in one shape: [v] is a number, a string, a bool, null, or
 * (graph, bars) a list of numbers.
 */
@Serializable
data class WidgetState(
	val v: JsonElement? = null,
	val text: String? = null,
	val level: String? = null,
	val blend: Double? = null,
	val max: Double? = null,
	val caption: String? = null,
	val labels: List<String>? = null,
	val levels: List<String?>? = null,
	val items: List<ItemState>? = null,
	val rows: List<Row>? = null,
	val total: Int? = null,
	val enabled: Boolean? = null,
	val players: List<Player>? = null,
	val volume: Volume? = null,
) {
	/** [v] as a number; null when it is missing or isn't one. */
	val number: Double? get() = v.number()

	/** [v] as a list of numbers, null where one is missing. */
	val numbers: List<Double?> get() = (v as? JsonArray)?.map { it.number() }.orEmpty()
}

@Serializable
data class ItemState(val v: JsonElement? = null, val text: String? = null, val level: String? = null)

/** A table's row: its [key], its level where its collection has one, and the fields its columns need. */
@Serializable
data class Row(
	val key: String,
	val level: String? = null,
	val blend: Double? = null,
	val f: JsonObject = JsonObject(emptyMap()),
)

/** A graph's (or a spark's) points so far: one time per point, one list per series, null where missing. */
@Serializable
data class HistoryEntry(val t: List<Long> = emptyList(), val v: List<List<Double?>> = emptyList())

@Serializable
data class Volume(
	/** % */
	val level: Double = 0.0,
	val muted: Boolean = false,
)

@Serializable
data class Player(
	val id: String,
	val name: String = "",
	/** "Playing", "Paused" or "Stopped". */
	val status: String = "Stopped",
	val title: String? = null,
	val artist: String? = null,
	val album: String? = null,
	/** Seconds. */
	val length: Double? = null,
	val position: Double? = null,
	/** % */
	val volume: Double? = null,
	@SerialName("has_art") val hasArt: Boolean = false,
	@SerialName("can_next") val canNext: Boolean = false,
	@SerialName("can_previous") val canPrevious: Boolean = false,
	@SerialName("can_play") val canPlay: Boolean = false,
	@SerialName("can_pause") val canPause: Boolean = false,
	@SerialName("can_seek") val canSeek: Boolean = false,
)

private val STREAM_TYPES = setOf("open", "data", "ui")
private val TYPE_PREFIX = Regex("""^\s*\{\s*"type"\s*:\s*"([^"]*)"""")

/**
 * One line of the stream. Null for a kind of line this app doesn't know, which a newer service
 * may send; a known kind that doesn't decode throws, so a broken line isn't read as zeros.
 */
fun decodeStreamLine(line: String, json: Json = ProtocolJson): StreamLine? {
	// The service puts the type first; reading it there saves parsing a whole line twice.
	val type = TYPE_PREFIX.find(line)?.groupValues?.get(1)
		?: json.parseToJsonElement(line).jsonObject["type"]?.jsonPrimitive?.content
	if (type !in STREAM_TYPES) return null
	return json.decodeFromString(StreamLine.serializer(), line)
}

// ---- requests --------------------------------------------------------------------------------

/** `POST /api/v1/stream/{id}`: what an open stream sends from its next line; each key optional. */
@Serializable
data class Subscribe(
	val dashboard: String? = null,
	val interval: Int? = null,
	/** A table's id to another of its `sorts`. */
	val sort: Map<String, String>? = null,
)

/** `POST /api/v1/actions/{id}`. */
@Serializable
data class ActionRequest(@EncodeDefault val params: JsonObject = JsonObject(emptyMap()))

/** What an action answers: for one the settings define, how its command ended and what it printed. */
@Serializable
data class ActionResult(
	val ok: Boolean = true,
	@SerialName("exit_code") val exitCode: Int? = null,
	@SerialName("timed_out") val timedOut: Boolean = false,
	val output: String? = null,
)

@Serializable
data class Ok(val ok: Boolean = true)

// ---- reading values ------------------------------------------------------------------------------

/** A JSON value as a number: a number, or null for anything else. */
fun JsonElement?.number(): Double? = (this as? JsonPrimitive)?.takeIf { !it.isString && it !is JsonNull }?.doubleOrNull

/** A JSON value as a bool, or null. */
fun JsonElement?.bool(): Boolean? = (this as? JsonPrimitive)?.takeIf { !it.isString && it !is JsonNull }?.booleanOrNull

/** A JSON value as text: a string as it is, a number or bool written out; null for null, a list or an object. */
fun JsonElement?.text(): String? = (this as? JsonPrimitive)?.takeIf { it !is JsonNull }?.content
