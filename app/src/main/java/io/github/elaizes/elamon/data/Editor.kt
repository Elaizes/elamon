package io.github.elaizes.elamon.data

import kotlinx.serialization.Serializable
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.intOrNull
import kotlinx.serialization.json.put

/*
 * The dashboard editor (elamond 0.11, docs/protocol.md "The dashboard editor"): what the service
 * says its forms may offer, the drafts the app makes of it, and what it sends back. The app keeps no
 * list of widget types, keys or values of its own: a type, a key or a kind it doesn't know is kept
 * as it is, and offered as JSON text.
 */

/** What `GET /api/v1/admin/editor` answers: everything the editor's forms may offer. */
@Serializable
data class EditorDescription(
	/** The editor's own drop-in, relative to the settings' folder. */
	val file: String = "",
	/** The editor's file's hash, what a write names as its `base`; null while there is none. */
	val hash: String? = null,
	val writable: Boolean = true,
	/** Why the editor's file can't be written, when it can't. */
	val why: String? = null,
	/** The asking phone's own profile. */
	val profile: String? = null,
	val dashboard: EditorDashboardKeys = EditorDashboardKeys(),
	/** The keys every widget takes. */
	val common: List<EditorKey> = emptyList(),
	val types: List<EditorType> = emptyList(),
	/** The keys of each table a `record` key names. */
	val records: Map<String, List<EditorKey>> = emptyMap(),
	val values: List<EditorValue> = emptyList(),
	val collections: List<EditorCollection> = emptyList(),
	val actions: List<EditorAction> = emptyList(),
	val links: List<EditorLink> = emptyList(),
	val sources: List<EditorSource> = emptyList(),
	val dashboards: List<EditorDashboard> = emptyList(),
	val profiles: List<EditorProfile> = emptyList(),
) {
	fun type(type: String?): EditorType? = types.find { it.type == type }
	fun dashboard(id: String?): EditorDashboard? = dashboards.find { it.id == id }
	fun profile(id: String?): EditorProfile? = profiles.find { it.id == id }

	/** The keys [widget] takes: the common ones, then those of its type, each once. */
	fun keysOf(widget: JsonObject): List<EditorKey> {
		val own = type(widget["type"].text())?.keys.orEmpty()
		val seen = HashSet<String>()
		return (common + own).filter { seen.add(it.key) }
	}

	/** How many of four columns [widget] takes: its own `span`, else its type's, else two. */
	fun spanOf(widget: JsonObject): Int =
		(widget["span"] as? JsonPrimitive)?.takeIf { !it.isString }?.intOrNull ?: type(widget["type"].text())?.span ?: DEFAULT_SPAN

	/** What a widget is called in the forms: its type's label, else the type as written. */
	fun typeLabel(widget: JsonObject): String {
		val t = widget["type"].text().orEmpty()
		return type(t)?.label?.ifBlank { null } ?: t
	}

	companion object {
		const val DEFAULT_SPAN = 2
	}
}

@Serializable
data class EditorDashboardKeys(val keys: List<EditorKey> = emptyList())

/**
 * A key a form offers: [kind] says what it holds and which more fields come with it (docs/protocol.md:
 * `min`, `max` of an `int`; `choices` of an `enum` or `format`; `to` of a `ref`; `of` of a `list`,
 * `map` or `any`; `record` of a `record`). An item's key (`of`) has no [key].
 */
@Serializable
data class EditorKey(
	val key: String = "",
	val kind: String = "",
	val label: String? = null,
	val hint: String? = null,
	val required: Boolean = false,
	/** What the service fills in while it isn't set. */
	val default: JsonElement? = null,
	val min: Double? = null,
	val max: Double? = null,
	val choices: List<JsonElement>? = null,
	/** What a `ref` names: `action`, `dashboard`, `collection`, `link` or `source`. */
	val to: String? = null,
	/** A `list`'s or a `map`'s item (an object); the keys an `any` may be one of (a list). */
	val of: JsonElement? = null,
	val record: String? = null,
) {
	/** Its name on screen: its label, else the key as written. */
	val name: String get() = label?.ifBlank { null } ?: key

	val choiceValues: List<String> get() = choices.orEmpty().mapNotNull { it.text() }

	/** A `list`'s or a `map`'s item. */
	val item: EditorKey? get() = (of as? JsonObject)?.let(::editorKey)

	/** What an `any` may be. */
	val alternatives: List<EditorKey> get() = (of as? JsonArray)?.mapNotNull { (it as? JsonObject)?.let(::editorKey) }.orEmpty()
}

/** A key of the description as the app reads it; one that doesn't read is a key of a kind no form knows. */
fun editorKey(o: JsonObject): EditorKey =
	runCatching { ProtocolJson.decodeFromJsonElement(EditorKey.serializer(), o) }.getOrElse { EditorKey(key = o["key"].text().orEmpty()) }

@Serializable
data class EditorType(
	val type: String,
	val label: String? = null,
	/** Its width while it doesn't say, of four columns. */
	val span: Int? = null,
	val keys: List<EditorKey> = emptyList(),
	/** What "Add widget" puts in. */
	val example: JsonObject? = null,
) {
	val name: String get() = label?.ifBlank { null } ?: type

	/** A new widget of this type: its example, or the type alone. */
	fun newWidget(): JsonObject = example?.takeIf { it["type"] != null } ?: JsonObject(mapOf("type" to JsonPrimitive(type)) + example.orEmpty())
}

/** A value there is, and its reading now as text. */
@Serializable
data class EditorValue(val address: String, val now: String? = null)

@Serializable
data class EditorCollection(val name: String, val fields: List<String> = emptyList())

@Serializable
data class EditorAction(val id: String, val name: String = "", val kind: String = "")

@Serializable
data class EditorLink(val id: String, val name: String = "")

@Serializable
data class EditorSource(val id: String, val kind: String = "")

/** A file of the settings and the line of it that set something. */
@Serializable
data class EditorFrom(val file: String = "", val line: Int? = null)

@Serializable
data class EditorDashboard(
	val id: String,
	val title: String = "",
	/** The dashboard as the settings merge it, before templates. */
	val def: JsonObject = JsonObject(emptyMap()),
	/** The files that set it, lowest first. */
	val from: List<EditorFrom> = emptyList(),
	/** The editor's file overrides it. */
	val editor: Boolean = false,
	/** Only the editor's file has it. */
	val own: Boolean = false,
)

@Serializable
data class EditorProfile(
	val id: String,
	val name: String = "",
	/** Its tabs, header and Android dashboards. */
	val def: JsonObject = JsonObject(emptyMap()),
	val from: List<EditorFrom> = emptyList(),
	val editor: Boolean = false,
	/** The paired clients that have it. */
	val clients: List<String> = emptyList(),
)

/** One thing wrong with the settings a write or a preview would make. */
@Serializable
data class Mistake(
	val file: String? = null,
	val line: Int? = null,
	/** What it is about, as the settings name it: `dashboards.mine.widgets[1].max`. */
	val path: String? = null,
	val message: String = "",
)

/** What a write of the editor answers. */
@Serializable
data class EditorSaved(
	val ok: Boolean = true,
	val hash: String? = null,
	val backup: String? = null,
	val generation: Long? = null,
)

/** What `POST /api/v1/admin/preview` answers: the `ui` with the draft, and a reading of its dashboard. */
data class Preview(val ui: UiDoc, val line: DataLine?)

/** Where a mistake is: a dashboard's widget (by its place, through groups) and key, a dashboard's key, a profile's key. */
sealed interface MistakeAt {
	/** In the dashboard [dashboard]: the widget at [widget] (none: the dashboard itself), its [key] (none: the whole of it). */
	data class InDashboard(val dashboard: String, val widget: List<Int>, val key: String?) : MistakeAt

	/** In the profile [profile], its [key] (`android.widget`; none: the whole of it). */
	data class InProfile(val profile: String, val key: String?) : MistakeAt

	data object Elsewhere : MistakeAt
}

/** A part of a settings path: a quoted key (one not of letters, digits, `_` and `-` alone, `"ups.eco"`), a bare key, a place `[2]`. */
private val PATH_PART = Regex(""""([^"]*)"|([^.\[\]"]+)|\[(\d+)]""")

/** Where [path] (`dashboards.mine.widgets[1].max`) points, in the editor's terms. */
fun mistakeAt(path: String?): MistakeAt {
	if (path.isNullOrBlank()) return MistakeAt.Elsewhere
	// Names as strings, places as numbers.
	val parts: List<Any> = PATH_PART.findAll(path).map { m -> m.groupValues[3].toIntOrNull() ?: m.groupValues[1].ifEmpty { m.groupValues[2] } }.toList()
	val table = parts.getOrNull(0) as? String
	val id = parts.getOrNull(1) as? String ?: return MistakeAt.Elsewhere
	return when (table) {
		"dashboards" -> {
			var rest = parts.drop(2)
			val widget = mutableListOf<Int>()
			while (rest.size >= 2 && rest[0] == WIDGETS && rest[1] is Int) {
				widget += rest[1] as Int
				rest = rest.drop(2)
			}
			MistakeAt.InDashboard(id, widget, rest.firstOrNull() as? String)
		}
		"profiles" -> MistakeAt.InProfile(id, parts.drop(2).filterIsInstance<String>().joinToString(".").ifEmpty { null })
		else -> MistakeAt.Elsewhere
	}
}

/** The mistakes a `bad_settings` answer carries; none for any other. */
fun mistakesOf(e: ElamondException): List<Mistake> {
	val list = e.body?.get("mistakes") as? JsonArray ?: return emptyList()
	return list.mapNotNull { m -> runCatching { ProtocolJson.decodeFromJsonElement(Mistake.serializer(), m) }.getOrNull() }
}

/** The key that holds a dashboard's or a group's widgets. */
const val WIDGETS = "widgets"

/** An id the settings take for a dashboard or a widget: 1–32 of a-z, 0-9, `_` and `-`. */
private val ID = Regex("[a-z0-9_-]{1,32}")

fun isSettingsId(s: String): Boolean = ID.matches(s)

/**
 * Changes to a draft: a dashboard (or a widget) as a JSON object, its widgets in `widgets`, a group's
 * in its own. A widget is found by its place, [path]: the place in the dashboard's widgets, then in
 * that group's, and so on. Only the keys asked for change; every other key stays as it was, where it
 * was, whether the app knows it or not.
 */
object Drafts {
	fun widgets(o: JsonObject): List<JsonElement> = (o[WIDGETS] as? JsonArray).orEmpty()

	/** The widget at [path] in [dashboard]; null where there is none, or it isn't an object. */
	fun widget(dashboard: JsonObject, path: List<Int>): JsonObject? {
		var at: JsonObject = dashboard
		for (i in path) at = widgets(at).getOrNull(i) as? JsonObject ?: return null
		return at
	}

	/** [o] with [key] set to [value]: in its place where it was, last where it wasn't; taken out with null. */
	fun withKey(o: JsonObject, key: String, value: JsonElement?): JsonObject {
		val m = LinkedHashMap(o)
		if (value == null) m.remove(key) else m[key] = value
		return JsonObject(m)
	}

	/** [dashboard] with the widget at [path] changed by [change]; the dashboard itself for an empty path. */
	fun change(dashboard: JsonObject, path: List<Int>, change: (JsonObject) -> JsonObject): JsonObject {
		if (path.isEmpty()) return change(dashboard)
		return changeList(dashboard, path.dropLast(1)) { list ->
			val i = path.last()
			val w = list.getOrNull(i) as? JsonObject ?: return@changeList
			list[i] = change(w)
		}
	}

	/** [dashboard] with the widgets of what is at [parent] (the dashboard for an empty one, else a group) changed by [change]. */
	fun changeList(dashboard: JsonObject, parent: List<Int>, change: (MutableList<JsonElement>) -> Unit): JsonObject {
		if (parent.isEmpty()) {
			val list = widgets(dashboard).toMutableList()
			change(list)
			return withKey(dashboard, WIDGETS, JsonArray(list))
		}
		return change(dashboard, parent) { group ->
			val list = widgets(group).toMutableList()
			change(list)
			withKey(group, WIDGETS, JsonArray(list))
		}
	}

	/** [widget] added to what is at [parent], at [at] (the end without it). */
	fun add(dashboard: JsonObject, parent: List<Int>, widget: JsonObject, at: Int? = null): JsonObject =
		changeList(dashboard, parent) { list -> list.add((at ?: list.size).coerceIn(0, list.size), widget) }

	/** The widget at [from] of what is at [parent] moved to [to]: in [to]'s place, which moves aside. */
	fun move(dashboard: JsonObject, parent: List<Int>, from: Int, to: Int): JsonObject =
		changeList(dashboard, parent) { list ->
			if (from !in list.indices || from == to) return@changeList
			val w = list.removeAt(from)
			list.add(to.coerceIn(0, list.size), w)
		}

	/** The widget at [path] taken out. */
	fun remove(dashboard: JsonObject, path: List<Int>): JsonObject {
		if (path.isEmpty()) return dashboard
		return changeList(dashboard, path.dropLast(1)) { list -> if (path.last() in list.indices) list.removeAt(path.last()) }
	}

	/** The widget at [path] (the dashboard for an empty one) with [key] set to [value], or taken out with null. */
	fun setKey(dashboard: JsonObject, path: List<Int>, key: String, value: JsonElement?): JsonObject =
		change(dashboard, path) { withKey(it, key, value) }

	/** The widget at [path] [span] columns wide; its type's own width again with null. */
	fun span(dashboard: JsonObject, path: List<Int>, span: Int?): JsonObject =
		setKey(dashboard, path, SPAN, span?.let { JsonPrimitive(it.coerceIn(1, Grid.COLUMNS)) })

	/** The keys of [widget] the description has no form for: kept as they are, offered as JSON text. */
	fun unknownKeys(widget: JsonObject, known: List<EditorKey>): List<String> {
		val names = known.map { it.key }.toSet()
		return widget.keys.filter { it !in names }
	}

	const val SPAN = "span"
}

/**
 * Which widgets of the preview's [dashboard] each of [draft]'s widgets became, in order. A widget is
 * `<dashboard>/<its id>`, or by its place without one, and `<…>.<item>` for each item of a
 * `for_each`; one the preview hasn't (not shown, as its `when` or the role decides) became none.
 * Where no id matches at all and there are as many of each, they are taken place by place.
 */
fun previewOf(draft: List<JsonElement>, dashboard: Dashboard?): List<List<Widget>> {
	val shown = dashboard?.widgets.orEmpty()
	val prefix = dashboard?.id.orEmpty() + "/"
	val taken = HashSet<String>()
	val byId = draft.mapIndexed { i, raw ->
		val own = ((raw as? JsonObject)?.get("id") as? JsonPrimitive)?.takeIf { it.isString }?.content
		val names = if (own != null) listOf(own) else listOf(i.toString(), "w$i")
		shown.filter { w ->
			val tail = w.id.removePrefix(prefix).takeIf { w.id.startsWith(prefix) } ?: return@filter false
			w.id !in taken && names.any { n -> tail == n || tail.startsWith("$n.") }
		}.also { found -> taken += found.map { it.id } }
	}
	if (byId.all { it.isEmpty() } && shown.size == draft.size) return shown.map { listOf(it) }
	return byId
}

// ---- requests ------------------------------------------------------------------------------------

const val ADMIN_PREVIEW_PATH = "/api/v1/admin/preview"
const val EDITOR_PATH = "/api/v1/admin/editor"
const val EDITOR_DASHBOARD_PATH = "$EDITOR_PATH/dashboard"
const val EDITOR_PROFILE_PATH = "$EDITOR_PATH/profile"
const val EDITOR_REVERT_PATH = "$EDITOR_PATH/revert"
const val EDITOR_DELETE_PATH = "$EDITOR_PATH/delete"

private fun base(hash: String?): JsonElement = hash?.let(::JsonPrimitive) ?: JsonNull

/** `POST /api/v1/admin/editor/dashboard`: the dashboard [id] written whole, as [dashboard] has it. */
fun dashboardBody(id: String, dashboard: JsonObject, hash: String?): JsonObject = buildJsonObject {
	put("id", id)
	put("dashboard", dashboard)
	put("base", base(hash))
}

/** `POST /api/v1/admin/editor/profile`: the profile [id]'s tabs, header and Android dashboards. */
fun profileBody(id: String, profile: JsonObject, hash: String?): JsonObject = buildJsonObject {
	put("id", id)
	put("profile", profile)
	put("base", base(hash))
}

/** What the editor's write of [table] (`dashboard` or `profile`) [id] is undone or taken out by: `…/revert`, `…/delete`. */
fun tableBody(table: String, id: String, hash: String?): JsonObject = buildJsonObject {
	put(table, id)
	put("base", base(hash))
}

/**
 * `POST /api/v1/admin/preview` for the editor: the drafts of [dashboards] and [profiles] (as the
 * editor's writes would send them, as one), and [dashboard] read with the widget types the app draws.
 */
fun previewBody(dashboards: Map<String, JsonObject>, profiles: Map<String, JsonObject>, dashboard: String): JsonObject = buildJsonObject {
	put(
		"editor",
		buildJsonObject {
			put("dashboards", JsonObject(dashboards))
			if (profiles.isNotEmpty()) put("profiles", JsonObject(profiles))
		},
	)
	put("dashboard", dashboard)
	put("widgets", WIDGET_TYPES.joinToString(","))
}

/** The service's own description of what the editor may offer. */
suspend fun ElamondClient.editor(): EditorDescription = get(EDITOR_PATH)

/** A write of the editor: [body] to [path]; its answer, or what refused it thrown, its mistakes and hash in its [ElamondException.body]. */
suspend fun ElamondClient.editorWrite(path: String, body: JsonObject): EditorSaved =
	decode(request("POST", path, body.toString(), finishOnCancel = true))

/**
 * What the computer would show with [body]'s drafts: the `ui` and a reading. A `ui` that leaves out
 * what the app keeps of the computer ([fallback]'s service, its control and profile) gets them from
 * there.
 */
suspend fun ElamondClient.preview(body: JsonObject, fallback: UiDoc?): Preview {
	val answer = decode<JsonObject>(request("POST", ADMIN_PREVIEW_PATH, body.toString()))
	return previewFrom(answer, fallback)
}

/** [answer] of a preview read as the app reads a `ui` and a data line; see [preview]. */
fun previewFrom(answer: JsonObject, fallback: UiDoc?): Preview {
	val raw = answer["ui"] as? JsonObject ?: throw ElamondException(ElamondException.Kind.Protocol, "no ui")
	val kept = fallback?.let { ProtocolJson.encodeToJsonElement(UiDoc.serializer(), it) as JsonObject }
	val filled = if (kept == null) raw else JsonObject(raw + listOf("service", "control", "profile").filter { it !in raw }.mapNotNull { k -> kept[k]?.let { k to it } })
	val ui = decodeChecked(UiDoc.serializer(), filled.toString())
	val line = (answer["data"] as? JsonObject)?.let { d ->
		runCatching { ProtocolJson.decodeFromJsonElement(DataLine.serializer(), d) }.getOrNull()
	}
	return Preview(ui, line)
}
