@file:OptIn(ExperimentalSerializationApi::class)

package io.github.elaizes.elamon.data

import kotlinx.serialization.DeserializationStrategy
import kotlinx.serialization.EncodeDefault
import kotlinx.serialization.ExperimentalSerializationApi
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.SerializationException
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
	/**
	 * The version of the dictionary of widget types and their keys: 1 from elamond 0.6, 2 from 0.7
	 * (`timeline`, `ranges`), 3 from 0.8 (`log`, row actions that open a dashboard). Nothing hangs on it: the service describes only the types the app
	 * lists ([WIDGET_TYPES]), and the app ignores keys it doesn't know.
	 */
	val widgets: Int? = null,
)

/** Pairing with a code; [previous], this phone's entry on that computer and its token, when it pairs again with one it has. */
@Serializable
data class PairRequest(
	val code: String,
	val device: String,
	val previous: PreviousPairing? = null,
)

/** The entry a phone had on a computer and its token: proof that it is the same phone pairing again. */
@Serializable
data class PreviousPairing(@SerialName("client_id") val clientId: String, val token: String)

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
	/** Where the computer is now: addresses a phone could reach it at, then names (16 at most). */
	val endpoints: List<String> = emptyList(),
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
	/** What this computer can offer at all, whatever this phone's role: told only to paired phones. */
	val features: List<String> = emptyList(),
	/** Where the computer is now: addresses a phone could reach it at, then names. */
	val endpoints: List<String> = emptyList(),
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
	/** Where its table is in the computer's settings, for an admin to change it; null for other roles. */
	val source: ProfileSource? = null,
)

/** A file of the computer's settings (from its settings folder when it is in it) and a line of it. */
@Serializable
data class ProfileSource(val file: String, val line: Int)

@Serializable
data class UnlockProfile(
	@SerialName("for") val kinds: List<String> = listOf("power", "nut", "signal", "actions", "units"),
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
	// graph, timeline
	val series: List<Series>? = null,
	/** Seconds back the graph or timeline reaches, live. */
	val range: Int? = null,
	/**
	 * The long ranges `/api/v1/history` answers for it ("1h", "24h", "7d", "30d", "1y", "all"),
	 * offered beside [range]; null from a service before 0.7, or a graph that offers none.
	 */
	val ranges: List<String>? = null,
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
	/** The fields a search looks in; the text columns when the service doesn't say. */
	@SerialName("search_fields") val searchFields: List<String>? = null,
	/** What the chips that ask for another order call each field. */
	@SerialName("sort_labels") val sortLabels: Map<String, String>? = null,
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
	// log (also `unit`, `level`, `search`)
	/** Whose journal a log shows: "system", "user", or "unit" (the journal of [unit]). */
	val scope: String? = null,
	/** How many of a log's newest lines the app keeps on screen. */
	val lines: Int? = null,
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

/**
 * What a table's row offers in its sheet: an [action] run with [params] filled from the row, or
 * (from elamond 0.8) a dashboard to [open], `<open>.<the row's key>`. One with neither is left out.
 */
@Serializable
data class RowAction(
	val action: String? = null,
	val label: String? = null,
	val params: JsonObject = JsonObject(emptyMap()),
	val open: String? = null,
)

@Serializable
data class Field(val field: String, val label: String? = null, val format: String? = null, val unit: String? = null)

/** An action of the computer's this client may run; [confirm] and [unlock] are final. */
@Serializable
data class Action(
	val id: String,
	val name: String = "",
	/** "power", "nut" (a station's), "signal", "actions", "units", "profile" or "media". */
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
data class Choice(
	val value: String,
	val label: String? = null,
	/** What the person types, exactly, before it runs (a station's command that cuts power), and the request carries. */
	@SerialName("confirm_text") val confirmText: String? = null,
)

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
data class UiChanged(
	val ui: String,
	/** Where the computer is now, as `ui` has them. */
	val endpoints: List<String> = emptyList(),
) : StreamLine

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
	/** A timeline's: its range back from the line's time, oldest first. */
	val segments: List<Segment>? = null,
	// log
	/** A log's lines since the last this stream sent, oldest first; with [reset], all it has. */
	val lines: List<LogLine>? = null,
	/** The lines replace what the app has: the first state on a stream, or after the filter changed. */
	val reset: Boolean = false,
	/** Lines left out before these. */
	val more: Int = 0,
	/** Why there may be more lines than these, or none: the journal can't be read whole, or at all. */
	val note: String? = null,
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

/**
 * A stretch of a timeline, Unix ms: "line" (on line, or on bypass), "battery", or "none" where
 * nothing is known; a state this app doesn't know is drawn as none.
 */
@Serializable
data class Segment(val from: Long, val to: Long, val state: String = "none")

/**
 * A line of the journal: [c] its time there in Unix µs, which the app only sends back to ask for the
 * lines before it; [t] the same in Unix ms; [unit] and [id] (the program) where the journal has them.
 */
@Serializable
data class LogLine(
	val c: Long,
	val t: Long,
	/** "debug", "info", "notice", "warning", "error" or "critical". */
	val level: String = "info",
	val unit: String? = null,
	val id: String? = null,
	val text: String = "",
)

// ---- GET /api/v1/history -------------------------------------------------------------------------

/**
 * A graph's long history over a range: buckets [stepMs] long from [from] to [to] (Unix ms), each
 * at its start in [t], the mean of each series in [v], null for a bucket without a point.
 */
@Serializable
data class GraphHistory(
	val widget: String,
	val range: String,
	val from: Long,
	val to: Long,
	@SerialName("step_ms") val stepMs: Long,
	val t: List<Long> = emptyList(),
	val v: List<List<Double?>> = emptyList(),
) {
	/** Its points, as a stream's history has them. */
	val entry: HistoryEntry get() = HistoryEntry(t, v)
}

/** A timeline over a range: its [segments] from [from] to [to] (Unix ms), as a stream's state has them. */
@Serializable
data class TimelineHistory(
	val widget: String,
	val range: String,
	val from: Long,
	val to: Long,
	val segments: List<Segment> = emptyList(),
)

// ---- GET /api/v1/log ---------------------------------------------------------------------------------

/** Older lines of a log, oldest first; [before] where to go on from, null once the journal has nothing older. */
@Serializable
data class LogPage(
	val widget: String,
	val lines: List<LogLine> = emptyList(),
	val before: Long? = null,
)

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

/** [text] read as [deserializer] says, or a [ElamondException.Kind.Protocol] error. */
fun <T> decodeChecked(deserializer: DeserializationStrategy<T>, text: String, json: Json = ProtocolJson): T = try {
	json.decodeFromString(deserializer, text)
} catch (e: IllegalArgumentException) {
	// SerializationException is one.
	throw ElamondException(ElamondException.Kind.Protocol, e.message, e)
}

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
	/** A log's id to the filter it is to send its lines by; null for its own. */
	val log: Map<String, LogFilter?>? = null,
)

/** What a log's lines are filtered by: the least [level], and text they hold (none when empty). Sent whole. */
@Serializable
data class LogFilter(val level: String, val search: String)

/** The least level a log shows while its widget doesn't say. */
const val DEFAULT_LOG_LEVEL = "info"

/** `POST /api/v1/actions/{id}`; [confirmText] only for a choice that asks for it, else left out. */
@Serializable
data class ActionRequest(
	@EncodeDefault val params: JsonObject = JsonObject(emptyMap()),
	@SerialName("confirm_text") val confirmText: String? = null,
)

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

fun JsonElement?.bool(): Boolean? = (this as? JsonPrimitive)?.takeIf { !it.isString && it !is JsonNull }?.booleanOrNull

/** A JSON value as text: a string as it is, a number or bool written out; null for null, a list or an object. */
fun JsonElement?.text(): String? = (this as? JsonPrimitive)?.takeIf { it !is JsonNull }?.content
