package io.github.elaizes.elamon

import android.app.Application
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.ViewModel
import androidx.lifecycle.ViewModelProvider
import androidx.lifecycle.viewModelScope
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Job
import kotlinx.coroutines.channels.BufferOverflow
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.collectLatest
import kotlinx.coroutines.flow.filterNotNull
import kotlinx.coroutines.flow.receiveAsFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import io.github.elaizes.elamon.data.DataLine
import io.github.elaizes.elamon.data.Drafts
import io.github.elaizes.elamon.data.EDITOR_DASHBOARD_PATH
import io.github.elaizes.elamon.data.EDITOR_DELETE_PATH
import io.github.elaizes.elamon.data.EDITOR_PROFILE_PATH
import io.github.elaizes.elamon.data.EDITOR_REVERT_PATH
import io.github.elaizes.elamon.data.EditorDescription
import io.github.elaizes.elamon.data.Limits
import io.github.elaizes.elamon.data.ElamondClient
import io.github.elaizes.elamon.data.ElamondException
import io.github.elaizes.elamon.data.ElamondException.Kind
import io.github.elaizes.elamon.data.MachineStore
import io.github.elaizes.elamon.data.Mistake
import io.github.elaizes.elamon.data.Preview
import io.github.elaizes.elamon.data.UiCache
import io.github.elaizes.elamon.data.UiDoc
import io.github.elaizes.elamon.data.WIDGETS
import io.github.elaizes.elamon.data.cleanName
import io.github.elaizes.elamon.data.dashboardBody
import io.github.elaizes.elamon.data.editor
import io.github.elaizes.elamon.data.editorWrite
import io.github.elaizes.elamon.data.mistakesOf
import io.github.elaizes.elamon.data.preview
import io.github.elaizes.elamon.data.previewBody
import io.github.elaizes.elamon.data.profileBody
import io.github.elaizes.elamon.data.tableBody
import io.github.elaizes.elamon.data.text

/** What the editor has open: a dashboard, or a profile's tabs. */
sealed interface Editing {
	/**
	 * The dashboard [id] of the settings, read on screen as [page] (one item of a `for_each`
	 * dashboard is `<id>.<item>`). [isNew] until it is first saved, then added to the tabs of the
	 * phone's own profile as well with [toTabs].
	 */
	data class Board(val id: String, val page: String = id, val isNew: Boolean = false, val toTabs: Boolean = false) : Editing

	/** The tabs, header and Android dashboards of the profile [profile]. */
	data class Tabs(val profile: String) : Editing
}

/** Another editor wrote the editor's file since it was read: [mine] whether what is open here is among what changed. */
data class EditorConflict(val mine: Boolean)

/** The preview: the `ui` with the draft and a reading of its dashboard, and its logs' lines. */
data class PreviewView(val preview: Preview, val logs: Map<String, LogLines>, val receivedAt: Long)

data class EditorState(
	val desc: EditorDescription? = null,
	val loading: Boolean = true,
	/** Why the description couldn't be read, or what is asked for isn't in it. */
	val failed: Message? = null,
	val editing: Editing? = null,
	/** The dashboard, or the profile's tabs, as changed here. */
	val draft: JsonObject = JsonObject(emptyMap()),
	/** The same as read, or as last saved: what leaving compares with. */
	val base: JsonObject = JsonObject(emptyMap()),
	val preview: PreviewView? = null,
	/** Why there is no preview, or an old one, now. */
	val previewNote: Message? = null,
	/** What the computer didn't take of the last write: until the next write. */
	val mistakes: List<Mistake> = emptyList(),
	/** What the last preview found wrong with the draft: until one finds nothing. */
	val previewMistakes: List<Mistake> = emptyList(),
	/** A write is on its way. */
	val busy: Boolean = false,
	val conflict: EditorConflict? = null,
	/** The dashboard was deleted: the editor closes. */
	val gone: Boolean = false,
) {
	/** Changes here the computer doesn't have. */
	val dirty: Boolean get() = draft != base || (editing as? Editing.Board)?.isNew == true

	/** Every mistake to show, the last write's and the preview's, each once. */
	val allMistakes: List<Mistake> get() = (mistakes + previewMistakes).distinct()
}

/**
 * The dashboard editor of a computer (elamond 0.11): what it offers, read from the computer; a draft
 * changed here, previewed by the computer as it changes; and written to the computer's settings when
 * saved. Nothing is written but on Save, Back to the files and Delete.
 */
class EditorViewModel(app: Application, private val machineId: String, private val opening: Editing?) : AndroidViewModel(app) {
	private val store = MachineStore.get(app)
	private val cache = UiCache.get(app)
	private val ticket = cache.ticket(machineId)
	private val _state = MutableStateFlow(EditorState())
	val state: StateFlow<EditorState> = _state.asStateFlow()

	private val _messages = Channel<Message>(capacity = Limits.MESSAGES_KEPT, onBufferOverflow = BufferOverflow.DROP_OLDEST)

	/** One-line results and errors for a snackbar. */
	val messages: Flow<Message> = _messages.receiveAsFlow()

	/** What the preview is asked for: changed, it is asked again, after a moment's quiet. */
	private val asked = MutableStateFlow<PreviewAsk?>(null)
	private var previewJob: Job? = null

	/** The graphs' and sparks' points in the preview, by widget id. */
	private val hists = HashMap<String, Hist>()

	/** The `ui` kept of the computer: its interval, until a preview has one. */
	private var kept: UiDoc? = null
	private var client: ElamondClient? = null

	private data class PreviewAsk(val dashboard: String, val draft: JsonObject)

	init {
		viewModelScope.launch {
			kept = cache.ui(machineId)
			load(opening)
		}
	}

	private fun say(m: Message) {
		_messages.trySend(m)
	}

	/** A client on whichever of the computer's addresses answers, with this phone's token: the same as every other request's. */
	private suspend fun client(): ElamondClient {
		client?.let { return it }
		val machine = store.get(machineId) ?: throw ElamondException(Kind.Unauthorized)
		return when (val r = reach(machine, store)) {
			is Reach.Reached -> r.client.also { client = it }
			Reach.Unpaired -> throw ElamondException(Kind.Unauthorized)
			Reach.Outdated -> throw ElamondException(Kind.Protocol)
			Reach.Offline -> throw ElamondException(Kind.Unreachable)
		}
	}

	/** [e] from a request: an address that stopped answering is looked for again next time. */
	private fun lost(e: Throwable) {
		if ((e as? ElamondException)?.kind == Kind.Unreachable) client = null
	}

	/** Reads what the editor offers, then opens [editing] in it (what is open now without it). */
	fun load(editing: Editing? = _state.value.editing) {
		_state.update { it.copy(loading = true, failed = null) }
		viewModelScope.launch {
			try {
				val desc = client().editor()
				_state.update { it.copy(desc = desc, loading = false) }
				if (editing != null) open(editing, desc)
			} catch (e: CancellationException) {
				throw e
			} catch (e: Exception) {
				lost(e)
				// With something open, it stays as it was read, and says why it wasn't read again.
				if (_state.value.editing != null) say(messageFor(e))
				_state.update { it.copy(loading = false, failed = messageFor(e)) }
			}
		}
	}

	/** Opens [editing] as the computer has it in [desc]. */
	private fun open(asked: Editing, desc: EditorDescription) {
		// The tabs of the phone's own profile, while it isn't named.
		val editing = if (asked is Editing.Tabs && asked.profile.isEmpty()) Editing.Tabs(desc.profile.orEmpty()) else asked
		val def = when (editing) {
			is Editing.Board -> desc.dashboard(editing.id)?.def
			is Editing.Tabs -> desc.profile(editing.profile)?.def
		}
		if (def == null) {
			val name = when (editing) {
				is Editing.Board -> editing.id
				is Editing.Tabs -> editing.profile
			}
			_state.update { it.copy(failed = Message.Res(R.string.editor_not_in_settings, listOf(name))) }
			return
		}
		_state.update { it.copy(editing = editing, draft = def, base = def, mistakes = emptyList(), conflict = null, failed = null) }
		ask()
	}

	/**
	 * A new dashboard [id] titled [title]: empty, or a copy of the dashboard [copyOf]; with [toTabs],
	 * added to the tabs of the phone's own profile when it is saved.
	 */
	fun startNew(id: String, title: String, copyOf: String?, toTabs: Boolean) {
		val desc = _state.value.desc ?: return
		val from = copyOf?.let { desc.dashboard(it)?.def }
		val draft = Drafts.withKey(from ?: JsonObject(mapOf(WIDGETS to JsonArray(emptyList()))), "title", JsonPrimitive(title))
		// The title first, as a new dashboard's table has it.
		val ordered = JsonObject(linkedMapOf<String, JsonElement>("title" to JsonPrimitive(title)) + draft)
		_state.update {
			it.copy(editing = Editing.Board(id, isNew = true, toTabs = toTabs && desc.profile != null), draft = ordered, base = ordered, mistakes = emptyList(), failed = null)
		}
		ask()
	}

	/** Opens the tabs of the profile [id] instead. */
	fun editTabs(id: String) {
		val desc = _state.value.desc ?: return
		open(Editing.Tabs(id), desc)
	}

	/** The draft changed by [change]. */
	fun change(change: (JsonObject) -> JsonObject) {
		_state.update { it.copy(draft = change(it.draft)) }
		ask()
	}

	/** The draft as it was: what an undo puts back. */
	fun restore(draft: JsonObject) {
		_state.update { it.copy(draft = draft) }
		ask()
	}

	/** Changes the draft to the computer's own copy again. */
	fun dropMine() {
		val s = _state.value
		val desc = s.desc ?: return
		val editing = s.editing ?: return
		_state.update { it.copy(conflict = null) }
		open(editing, desc)
	}

	// ---- the preview ----------------------------------------------------------------------------

	/** The preview asked for again, for the draft as it is now. */
	private fun ask() {
		val s = _state.value
		val b = s.editing as? Editing.Board ?: run {
			asked.value = null
			return
		}
		asked.value = PreviewAsk(b.id, s.draft)
	}

	/**
	 * The screen is shown: the computer previews the draft, at once, again after each change has
	 * settled, and every interval meanwhile, as a stream's readings come. Not shown, it stops.
	 */
	fun setVisible(visible: Boolean) {
		if (!visible) {
			previewJob?.cancel()
			previewJob = null
			return
		}
		if (previewJob?.isActive == true) return
		previewJob = viewModelScope.launch {
			asked.filterNotNull().settle(PREVIEW_SETTLE_MS).collectLatest { a ->
				// Places change with the draft: the points of the widget there before aren't this one's.
				hists.clear()
				while (previewOnce(a)) delay(intervalMs().toLong())
			}
		}
	}

	private fun intervalMs(): Int {
		val ui = _state.value.preview?.preview?.ui ?: kept
		return ui?.let { Intervals.effective(it.profile.intervalMs, it.service.intervalMs) } ?: Intervals.DEFAULT_MS
	}

	/** Asks for one preview of [a]; false when asking again won't change the answer. */
	private suspend fun previewOnce(a: PreviewAsk): Boolean {
		return try {
			// Asked for by its id in the settings, as the computer takes it.
			val p = client().preview(previewBody(mapOf(a.dashboard to a.draft), emptyMap(), a.dashboard))
			p.line?.let { takeHists(p.ui, it) }
			val logs = p.line?.let { line -> logsOf(p.ui, line) }.orEmpty()
			_state.update { it.copy(preview = PreviewView(p, logs, System.currentTimeMillis()), previewNote = null, previewMistakes = emptyList()) }
			true
		} catch (e: CancellationException) {
			throw e
		} catch (e: Exception) {
			lost(e)
			val ex = e as? ElamondException
			when {
				// The last preview that worked stays, the mistakes beside it.
				ex?.code == "bad_settings" -> _state.update { it.copy(previewMistakes = mistakesOf(ex), previewNote = Message.Res(R.string.editor_preview_mistakes)) }
				// Nothing the role sees in it yet (an empty dashboard), or one made for each item of a
				// collection, which the computer has under its items' ids alone.
				ex?.code == "no_such_dashboard" -> {
					val each = a.draft["for_each"] != null
					_state.update { it.copy(preview = null, previewNote = Message.Res(if (each) R.string.editor_preview_for_each else R.string.editor_preview_empty)) }
				}
				else -> _state.update { it.copy(previewNote = messageFor(e)) }
			}
			// Refused: not again until something changes.
			ex == null || ex.kind == Kind.Unreachable || ex.status >= SERVER_ERRORS
		}
	}

	/** The preview's graphs and sparks, as a stream's are kept: its history replaced, a point added each reading. */
	private fun takeHists(ui: UiDoc, line: DataLine) {
		val dash = ui.dashboard(line.dashboard) ?: return
		val step = intervalMs().toLong()
		for (w in allWidgets(dash.widgets)) {
			val graph = w.type == "graph"
			if (!graph && !(w.type == "value" && w.spark == true)) continue
			val count = if (graph) w.series?.size?.coerceAtLeast(1) ?: 1 else 1
			val window = ((if (graph) w.range else null)?.takeIf { it > 0 } ?: ui.service.historySeconds) * MS_PER_SECOND
			val h = hists[w.id]?.takeIf { it.seriesCount == count } ?: Hist(count, window).also { hists[w.id] = it }
			h.windowMs = window
			h.stepHintMs = step
			val s = line.widgets[w.id]
			h.take(line.t, if (graph) s?.numbers.orEmpty() else listOf(s?.number), line.history?.get(w.id))
		}
	}

	/** The points of the preview's graph or spark [widget]. */
	fun hist(widget: String): Hist? = hists[widget]

	/** The lines of the preview's logs, as the reading has them. */
	private fun logsOf(ui: UiDoc, line: DataLine): Map<String, LogLines> {
		val dash = ui.dashboard(line.dashboard) ?: return emptyMap()
		return allWidgets(dash.widgets).filter { it.type == "log" }.mapNotNull { w ->
			line.widgets[w.id]?.let { s -> w.id to LogLines().take(s, w.lines ?: Limits.LOG_LINES_DEFAULT) }
		}.toMap()
	}

	// ---- writing --------------------------------------------------------------------------------

	/** Writes the draft to the computer's settings: the dashboard (and a new one into the tabs), or the profile's tabs. */
	fun save() {
		val s = _state.value
		val desc = s.desc ?: return
		val editing = s.editing ?: return
		if (s.busy) return
		val draft = s.draft
		_state.update { it.copy(busy = true) }
		viewModelScope.launch {
			try {
				val c = client()
				var tabbed = true
				when (editing) {
					is Editing.Board -> {
						val r = c.editorWrite(EDITOR_DASHBOARD_PATH, dashboardBody(editing.id, draft, desc.hash))
						_state.update { it.copy(base = draft, editing = editing.copy(isNew = false, toTabs = false)) }
						if (editing.isNew && editing.toTabs) tabbed = intoTabs(c, desc, editing.id, r.hash)
					}
					is Editing.Tabs -> {
						c.editorWrite(EDITOR_PROFILE_PATH, profileBody(editing.profile, draft, desc.hash))
						_state.update { it.copy(base = draft) }
					}
				}
				_state.update { it.copy(mistakes = emptyList(), conflict = null) }
				// The dashboard is written either way; only its tab may not be.
				say(Message.Res(if (tabbed) R.string.editor_saved else R.string.editor_saved_no_tab))
				afterWrite(c)
			} catch (e: CancellationException) {
				throw e
			} catch (e: Exception) {
				refused(e)
			} finally {
				_state.update { it.copy(busy = false) }
			}
		}
	}

	/** The new dashboard [id] added to the tabs of the phone's own profile, after the editor's file got [hash]; whether it was. */
	private suspend fun intoTabs(c: ElamondClient, desc: EditorDescription, id: String, hash: String?): Boolean {
		val own = desc.profile(desc.profile) ?: return false
		val tabs = (own.def["dashboards"] as? JsonArray)?.toList() ?: tabsKept()
		if (tabs.any { it.text() == id }) return true
		val profile = Drafts.withKey(own.def, "dashboards", JsonArray(tabs + JsonPrimitive(id)))
		return try {
			c.editorWrite(EDITOR_PROFILE_PATH, profileBody(own.id, profile, hash))
			true
		} catch (e: CancellationException) {
			throw e
		} catch (e: Exception) {
			lost(e)
			false
		}
	}

	/** The tabs as the computer shows this phone, one for each dashboard of the settings. */
	private fun tabsKept(): List<JsonElement> = kept?.tabs.orEmpty().map { it.substringBefore('.') }.distinct().map(::JsonPrimitive)

	/**
	 * After a write: what the editor offers read again (the file's new hash), and the computer's `ui`
	 * for this phone kept, as the stream would have it: the home-screen widget, the tiles and the card
	 * read the one kept.
	 */
	private suspend fun afterWrite(c: ElamondClient) {
		try {
			val desc = c.editor()
			_state.update { it.copy(desc = desc) }
		} catch (e: CancellationException) {
			throw e
		} catch (e: Exception) {
			lost(e)
		}
		orNull {
			val ui = c.ui()
			val clean = ui.copy(service = ui.service.copy(name = cleanName(ui.service.name)))
			cache.putUi(machineId, clean, ticket)
			kept = clean
		}
	}

	/** What refused a write, told: another editor's write (409) read again and kept apart, mistakes beside what they are about. */
	private suspend fun refused(e: Exception) {
		lost(e)
		val ex = e as? ElamondException
		when {
			ex?.code == "changed_on_disk" -> {
				val before = _state.value
				val fresh = try {
					client().editor()
				} catch (x: CancellationException) {
					throw x
				} catch (x: Exception) {
					null
				}
				if (fresh != null) {
					val mine = before.editing?.let { defOf(before.desc, it) != defOf(fresh, it) } ?: true
					_state.update { it.copy(desc = fresh, conflict = EditorConflict(mine)) }
				}
				say(Message.Res(R.string.err_changed_on_disk))
			}
			ex?.code == "bad_settings" -> {
				val found = mistakesOf(ex)
				_state.update { it.copy(mistakes = found) }
				say(Message.Res(R.string.editor_mistakes))
			}
			// A body over what the computer takes: a plain-text answer.
			ex?.status == HTTP_TOO_LARGE -> say(Message.Res(R.string.editor_too_large))
			else -> say(messageFor(e))
		}
	}

	private fun defOf(desc: EditorDescription?, editing: Editing): JsonObject? = when (editing) {
		is Editing.Board -> desc?.dashboard(editing.id)?.def
		is Editing.Tabs -> desc?.profile(editing.profile)?.def
	}

	/** Takes what is open out of the editor's file: back to what the other files say. */
	fun revert() = undo(EDITOR_REVERT_PATH)

	/** Takes the dashboard only the editor's file has out of it, and out of the tabs the editor wrote. */
	fun delete() = undo(EDITOR_DELETE_PATH)

	private fun undo(path: String) {
		val s = _state.value
		val desc = s.desc ?: return
		val editing = s.editing ?: return
		if (s.busy) return
		val body = when (editing) {
			is Editing.Board -> tableBody("dashboard", editing.id, desc.hash)
			is Editing.Tabs -> tableBody("profile", editing.profile, desc.hash)
		}
		_state.update { it.copy(busy = true) }
		viewModelScope.launch {
			try {
				val c = client()
				c.editorWrite(path, body)
				afterWrite(c)
				val fresh = _state.value.desc
				val def = defOf(fresh, editing)
				if (path == EDITOR_DELETE_PATH || def == null) {
					_state.update { it.copy(gone = true) }
				} else {
					_state.update { it.copy(draft = def, base = def, mistakes = emptyList(), conflict = null) }
					ask()
				}
				say(Message.Res(if (path == EDITOR_DELETE_PATH) R.string.editor_deleted else R.string.editor_reverted))
			} catch (e: CancellationException) {
				throw e
			} catch (e: Exception) {
				refused(e)
			} finally {
				_state.update { it.copy(busy = false) }
			}
		}
	}

	override fun onCleared() {
		previewJob?.cancel()
	}

	class Factory(private val app: Application, private val id: String, private val opening: Editing?) : ViewModelProvider.Factory {
		@Suppress("UNCHECKED_CAST")
		override fun <T : ViewModel> create(modelClass: Class<T>): T = EditorViewModel(app, id, opening) as T
	}

	private companion object {
		/** How long the draft stays the same before the computer previews it again. */
		const val PREVIEW_SETTLE_MS = 400L
		const val MS_PER_SECOND = 1000L
		const val SERVER_ERRORS = 500
		const val HTTP_TOO_LARGE = 413
	}
}
