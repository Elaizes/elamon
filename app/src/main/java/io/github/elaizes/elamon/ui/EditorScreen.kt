package io.github.elaizes.elamon.ui

import android.app.Application
import androidx.activity.compose.BackHandler
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.detectDragGesturesAfterLongPress
import androidx.compose.foundation.gestures.scrollBy
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.navigationBars
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBars
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.filled.Add
import androidx.compose.material.icons.filled.Delete
import androidx.compose.material.icons.filled.KeyboardArrowDown
import androidx.compose.material.icons.filled.KeyboardArrowUp
import androidx.compose.material.icons.filled.Menu
import androidx.compose.material.icons.filled.MoreVert
import androidx.compose.material.icons.filled.Warning
import androidx.compose.material3.Button
import androidx.compose.material3.Checkbox
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.ExtendedFloatingActionButton
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.SnackbarHost
import androidx.compose.material3.SnackbarHostState
import androidx.compose.material3.SnackbarResult
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.rememberModalBottomSheetState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.runtime.withFrameNanos
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.hapticfeedback.HapticFeedbackType
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.layout.boundsInRoot
import androidx.compose.ui.layout.onGloballyPositioned
import androidx.compose.ui.layout.positionInRoot
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalHapticFeedback
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.CustomAccessibilityAction
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.customActions
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.ui.unit.dp
import androidx.compose.ui.zIndex
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.LifecycleEventObserver
import androidx.lifecycle.ViewModelStoreOwner
import androidx.lifecycle.compose.LocalLifecycleOwner
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewmodel.compose.viewModel
import dev.chrisbanes.haze.HazeState
import dev.chrisbanes.haze.hazeSource
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonObject
import io.github.elaizes.elamon.ActionsState
import io.github.elaizes.elamon.Conn
import io.github.elaizes.elamon.EditorState
import io.github.elaizes.elamon.EditorViewModel
import io.github.elaizes.elamon.Editing
import io.github.elaizes.elamon.Guarded
import io.github.elaizes.elamon.Hist
import io.github.elaizes.elamon.MachineState
import io.github.elaizes.elamon.MachineViewModel
import io.github.elaizes.elamon.R
import io.github.elaizes.elamon.Settings
import io.github.elaizes.elamon.UnlockGrace
import io.github.elaizes.elamon.data.Dashboard
import io.github.elaizes.elamon.data.Drafts
import io.github.elaizes.elamon.data.EditorDescription
import io.github.elaizes.elamon.data.EditorFrom
import io.github.elaizes.elamon.data.EditorKey
import io.github.elaizes.elamon.data.Grid
import io.github.elaizes.elamon.data.Machine
import io.github.elaizes.elamon.data.Mistake
import io.github.elaizes.elamon.data.WIDGETS
import io.github.elaizes.elamon.data.MistakeAt
import io.github.elaizes.elamon.data.Widget
import io.github.elaizes.elamon.data.isSettingsId
import io.github.elaizes.elamon.data.mistakeAt
import io.github.elaizes.elamon.data.previewOf
import io.github.elaizes.elamon.data.text

/** Where the editor's sheet is open: the widget's place, as text a rotation keeps. */
private fun pathOf(s: String?): List<Int>? = s?.split('/')?.mapNotNull { it.toIntOrNull() }?.takeIf { it.isNotEmpty() }

private fun keyOf(path: List<Int>): String = path.joinToString("/")

/** How near the top or bottom of the list a dragged widget scrolls it, and how far a frame. */
private val EDGE = 72.dp
private val STEP = 12.dp

/** A widget being dragged lifts this much over the others. */
private const val LIFT = 1.03f
private const val LIFTED_ALPHA = 0.92f
private val LIFT_SHADOW = 8.dp
private val MARK = 2.dp

/** Room under the list for the Add widget button. */
private val FAB_ROOM = 96.dp

/**
 * The dashboard editor of a computer (elamond 0.11): the dashboard [dashboard] (a page's id as the
 * computer's screen names it), a new one without it, or with [tabs] the tabs of a profile. The
 * dashboard is drawn by the computer's preview with the same code as on its screen; each widget is
 * moved by its handle and changed in a sheet with the fields the computer describes.
 */
@Composable
fun EditorScreen(
	machine: Machine,
	dashboard: String?,
	tabs: Boolean,
	owner: ViewModelStoreOwner,
	/** The computer's screen's own store: its view model is what the preview's widgets are drawn with. */
	machineOwner: ViewModelStoreOwner,
	onBack: () -> Unit,
	onTabs: () -> Unit,
	onHelp: () -> Unit,
) {
	val ctx = LocalContext.current
	val app = ctx.applicationContext as Application
	val opening = when {
		tabs -> Editing.Tabs("")
		dashboard != null -> Editing.Board(dashboard.substringBefore('.'), page = dashboard)
		else -> null
	}
	val vm: EditorViewModel = viewModel(viewModelStoreOwner = owner, factory = EditorViewModel.Factory(app, machine.id, opening))
	val machineVm: MachineViewModel = viewModel(viewModelStoreOwner = machineOwner, factory = MachineViewModel.Factory(app, machine.id))
	val state by vm.state.collectAsStateWithLifecycle()
	val machineState by machineVm.state.collectAsStateWithLifecycle()
	val snackbar = remember { SnackbarHostState() }
	val haze = remember { HazeState() }
	val scope = rememberCoroutineScope()
	val guard = LocalGuard.current
	val res by rememberUpdatedState(resources())
	val name = machineState.ui?.service?.name?.ifEmpty { null } ?: machine.name

	// The preview while the screen can be seen, as a computer's stream.
	val lifecycle = LocalLifecycleOwner.current.lifecycle
	DisposableEffect(lifecycle) {
		val observer = LifecycleEventObserver { _, e ->
			when (e) {
				Lifecycle.Event.ON_START -> vm.setVisible(true)
				Lifecycle.Event.ON_STOP -> vm.setVisible(false)
				else -> {}
			}
		}
		lifecycle.addObserver(observer)
		if (lifecycle.currentState.isAtLeast(Lifecycle.State.STARTED)) vm.setVisible(true)
		onDispose {
			lifecycle.removeObserver(observer)
			vm.setVisible(false)
		}
	}
	LaunchedEffect(Unit) { vm.messages.collect { snackbar.showSnackbar(it.text(res)) } }
	LaunchedEffect(guard) { guard.messages.collect { snackbar.showSnackbar(it) } }
	LaunchedEffect(state.gone) { if (state.gone) onBack() }

	var leaving by rememberSaveable { mutableStateOf(false) }
	var confirm by rememberSaveable { mutableStateOf<String?>(null) }
	var menu by remember { mutableStateOf(false) }
	var keysOpen by rememberSaveable { mutableStateOf(false) }
	var adding by rememberSaveable { mutableStateOf(false) }
	var sheetAt by rememberSaveable { mutableStateOf<String?>(null) }
	val back = { if (state.dirty) leaving = true else onBack() }
	BackHandler(enabled = state.dirty) { leaving = true }

	// Writing the settings unlocks the phone first, whatever the profile asks for actions: it can change what every client sees.
	val prompt = stringResource(R.string.editor_unlock_prompt, name)
	val question = stringResource(R.string.editor_write_question)
	val writeWord = stringResource(R.string.editor_save)
	val write = { button: String, run: () -> Unit ->
		guard.run(Ask(Guarded.Power, prompt = prompt, question = question, button = button, confirm = false, unlock = true, grace = UnlockGrace.EveryTime)) { run() }
	}
	val save = { write(writeWord) { vm.save() } }

	val desc = state.desc
	val editing = state.editing
	val board = editing as? Editing.Board
	val profile = (editing as? Editing.Tabs)?.let { desc?.profile(it.profile) }
	val title = when {
		board != null -> state.draft["title"].text()?.ifBlank { null } ?: board.id
		profile != null -> stringResource(R.string.editor_tabs_title, profile.name.ifBlank { profile.id })
		tabs -> stringResource(R.string.editor_menu_tabs)
		dashboard == null -> stringResource(R.string.editor_new_title)
		else -> stringResource(R.string.editor_menu_edit)
	}
	val lower = editing?.let { lowerFiles(desc, it) }.orEmpty()
	val env = desc?.let { d -> FormEnv(d) { path -> sheetAt = keyOf(path) } }

	CompositionLocalProvider(LocalHaze provides haze) {
		Box(Modifier.fillMaxSize()) {
			Box(Modifier.fillMaxSize().hazeSource(haze)) {
				when {
					state.loading -> Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) { CircularProgressIndicator() }
					state.failed != null && editing == null -> Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
						Notice(Icons.Default.Warning, stringResource(R.string.editor_failed), state.failed?.text(resources())) {
							Button(onClick = { vm.load(opening) }) { Text(stringResource(R.string.retry)) }
						}
					}
					desc == null || env == null || editing == null -> Box(Modifier.fillMaxSize())
					board != null -> BoardEditor(state, desc, board, env, machineVm, machineState, machine.id, name, lower, vm::hist, onOpen = { sheetAt = keyOf(it) }, onMove = { from, to -> vm.change { Drafts.move(it, emptyList(), from, to) } }, onTakeTheirs = { vm.dropMine() }, onSaveMine = save)
					else -> TabsEditor(state, desc, editing as Editing.Tabs, env, lower, onKey = { k, v -> vm.change { d -> Drafts.withKey(d, k, v) } }, onAndroid = { k, v ->
						vm.change { d -> Drafts.withKey(d, ANDROID, Drafts.withKey(d[ANDROID] as? JsonObject ?: JsonObject(emptyMap()), k, v)) }
					}, onProfile = { vm.editTabs(it) }, onTakeTheirs = { vm.dropMine() }, onSaveMine = save)
				}
			}

			GlassTopBar(
				title = title,
				collapsed = true,
				onBack = back,
				subtitle = name,
				actions = {
					if (editing != null && desc != null) {
						TextButton(onClick = save, enabled = state.dirty && !state.busy && desc.writable) {
							if (state.busy) CircularProgressIndicator(Modifier.size(Sizes.iconM), strokeWidth = Sizes.spinnerStroke)
							else Text(writeWord)
						}
					}
					IconButton(onClick = { menu = true }) { Icon(Icons.Default.MoreVert, stringResource(R.string.machine_more)) }
					DropdownMenu(expanded = menu, onDismissRequest = { menu = false }) {
						if (board != null) {
							DropdownMenuItem(text = { Text(stringResource(R.string.editor_dashboard_keys)) }, onClick = { menu = false; keysOpen = true })
							DropdownMenuItem(text = { Text(stringResource(R.string.editor_menu_tabs)) }, onClick = { menu = false; onTabs() })
						}
						val known = when (editing) {
							is Editing.Board -> desc?.dashboard(editing.id)?.takeIf { !editing.isNew }
							else -> null
						}
						if (known?.editor == true && !known.own) {
							DropdownMenuItem(text = { Text(stringResource(R.string.editor_revert)) }, onClick = { menu = false; confirm = REVERT })
						}
						if (profile?.editor == true) {
							DropdownMenuItem(text = { Text(stringResource(R.string.editor_revert)) }, onClick = { menu = false; confirm = REVERT })
						}
						if (known?.own == true) {
							DropdownMenuItem(text = { Text(stringResource(R.string.editor_delete)) }, onClick = { menu = false; confirm = DELETE })
						}
						// A new dashboard is the phone's alone until it is saved: nothing to read again.
						if (editing != null && board?.isNew != true) {
							DropdownMenuItem(text = { Text(stringResource(R.string.editor_reload)) }, onClick = { menu = false; if (state.dirty) confirm = RELOAD else vm.load() })
						}
						DropdownMenuItem(text = { Text(stringResource(R.string.help)) }, onClick = { menu = false; onHelp() })
					}
				},
			)

			if (board != null && desc != null) {
				ExtendedFloatingActionButton(
					onClick = { adding = true },
					icon = { Icon(Icons.Default.Add, null) },
					text = { Text(stringResource(R.string.editor_add)) },
					modifier = Modifier.align(Alignment.BottomEnd).navigationBarsPadding().padding(Space.l),
				)
			}
			SnackbarHost(snackbar, Modifier.align(Alignment.BottomCenter).navigationBarsPadding().padding(bottom = FAB_ROOM))
		}
	}

	// ---- what opens over it ----------------------------------------------------------------------

	// A new dashboard: its id and title first.
	if (desc != null && editing == null && !state.loading && dashboard == null && !tabs) {
		NewDashboardDialog(desc, onDismiss = onBack) { id, t, copy, toTabs -> vm.startNew(id, t, copy, toTabs) }
	}
	if (adding && desc != null && board != null) {
		AddWidgetDialog(desc, onDismiss = { adding = false }) { w ->
			vm.change { Drafts.add(it, emptyList(), w) }
			scope.launch { snackbar.showSnackbar(res.getString(R.string.editor_added, desc.typeLabel(w))) }
		}
	}
	val sheetPath = pathOf(sheetAt)
	if (sheetPath != null && desc != null && env != null && board != null) {
		val removed = stringResource(R.string.editor_removed)
		val undo = stringResource(R.string.editor_undo)
		WidgetSheet(
			sheetPath, state, desc, board.id, env,
			onKey = { k, v -> vm.change { Drafts.setKey(it, sheetPath, k, v) } },
			onMove = { by ->
				val i = sheetPath.last()
				vm.change { Drafts.move(it, sheetPath.dropLast(1), i, i + by) }
				sheetAt = keyOf(sheetPath.dropLast(1) + (i + by))
			},
			onRemove = {
				val w = Drafts.widget(state.draft, sheetPath)
				vm.change { Drafts.remove(it, sheetPath) }
				val parent = sheetPath.dropLast(1)
				sheetAt = parent.takeIf { it.isNotEmpty() }?.let(::keyOf)
				if (w != null) scope.launch {
					if (snackbar.showSnackbar(removed, actionLabel = undo, withDismissAction = true) == SnackbarResult.ActionPerformed) {
						vm.change { Drafts.add(it, parent, w, sheetPath.last()) }
					}
				}
			},
			onOpen = { sheetAt = it?.let(::keyOf) },
		)
	}
	if (keysOpen && desc != null && env != null && board != null) {
		DashboardKeysSheet(state, desc, board.id, env, lower, onKey = { k, v -> vm.change { Drafts.withKey(it, k, v) } }, onDismiss = { keysOpen = false })
	}

	if (leaving) {
		GlassDialog(
			onDismiss = { leaving = false },
			title = stringResource(R.string.editor_leave_title),
			text = { Text(stringResource(R.string.editor_leave_body)) },
			confirm = { TextButton(onClick = { leaving = false; onBack() }) { Text(stringResource(R.string.editor_leave)) } },
			dismiss = { TextButton(onClick = { leaving = false }) { Text(stringResource(R.string.editor_keep)) } },
		)
	}
	when (confirm) {
		REVERT -> {
			val button = stringResource(R.string.editor_revert)
			GlassDialog(
				onDismiss = { confirm = null },
				title = stringResource(R.string.editor_revert_title),
				text = { Text(stringResource(if (board != null) R.string.editor_revert_body else R.string.editor_revert_tabs_body, desc?.file.orEmpty())) },
				confirm = { TextButton(onClick = { confirm = null; write(button) { vm.revert() } }) { Text(button) } },
				dismiss = { TextButton(onClick = { confirm = null }) { Text(stringResource(R.string.cancel)) } },
			)
		}
		DELETE -> {
			val button = stringResource(R.string.editor_delete_confirm)
			GlassDialog(
				onDismiss = { confirm = null },
				title = stringResource(R.string.editor_delete_title, title),
				text = { Text(stringResource(R.string.editor_delete_body, desc?.file.orEmpty())) },
				confirm = { TextButton(onClick = { confirm = null; write(button) { vm.delete() } }) { Text(button) } },
				dismiss = { TextButton(onClick = { confirm = null }) { Text(stringResource(R.string.cancel)) } },
			)
		}
		RELOAD -> GlassDialog(
			onDismiss = { confirm = null },
			title = stringResource(R.string.editor_reload_title),
			text = { Text(stringResource(R.string.editor_reload_body)) },
			confirm = { TextButton(onClick = { confirm = null; vm.load() }) { Text(stringResource(R.string.editor_reload)) } },
			dismiss = { TextButton(onClick = { confirm = null }) { Text(stringResource(R.string.cancel)) } },
		)
	}
}

private const val REVERT = "revert"
private const val DELETE = "delete"
private const val RELOAD = "reload"
private const val ANDROID = "android"
private const val TYPE = "type"

/**
 * The files below the editor's own that set what [editing] opens: what they set can be changed,
 * not taken out (the files merge key by key).
 */
private fun lowerFiles(desc: EditorDescription?, editing: Editing): List<EditorFrom> {
	desc ?: return emptyList()
	val from = when (editing) {
		is Editing.Board -> desc.dashboard(editing.id)?.from
		is Editing.Tabs -> desc.profile(editing.profile)?.from
	}
	return from.orEmpty().filter { it.file != desc.file }
}

/** The keys of [base] (what the computer has) a lower file may have set, named by their files; none without one. */
private fun fixedBy(lower: List<EditorFrom>, base: JsonObject): (String) -> String? {
	if (lower.isEmpty()) return { null }
	val files = lower.map { it.file }.distinct().joinToString(", ")
	return { key -> files.takeIf { key in base } }
}

/** What the computer found wrong in the dashboard [board]: by widget place, its keys and the dashboard's own, and the rest. */
private class BoardMistakes(all: List<Mistake>, private val board: String) {
	private val placed = all.map { it to mistakeAt(it.path) }
	private fun inBoard(a: MistakeAt): MistakeAt.InDashboard? = (a as? MistakeAt.InDashboard)?.takeIf { it.dashboard == board }

	/** Those of the widget at [i] of the dashboard, or of any in it. */
	fun ofCell(i: Int): List<String> = placed.filter { (_, a) -> inBoard(a)?.widget?.firstOrNull() == i }.map { it.first.message }

	/** Those of the widget at [path], by key ("" for the whole of it); those of a widget in it, under its `widgets`. */
	fun ofWidget(path: List<Int>): Map<String, List<String>> = placed.mapNotNull { (m, a) ->
		val d = inBoard(a) ?: return@mapNotNull null
		when {
			d.widget == path -> (d.key ?: "") to m.message
			d.widget.size > path.size && d.widget.take(path.size) == path -> WIDGETS to m.message
			else -> null
		}
	}.groupBy({ it.first }, { it.second })

	/** Those of the dashboard's own keys. */
	val ofKeys: Map<String, List<String>> = placed.mapNotNull { (m, a) -> inBoard(a)?.takeIf { it.widget.isEmpty() }?.let { (it.key ?: "") to m.message } }.groupBy({ it.first }, { it.second })

	/** Those of anything else: another dashboard, a profile, the settings as a whole. */
	val elsewhere: List<Mistake> = placed.filter { (_, a) -> inBoard(a) == null }.map { it.first }
}

/** A mistake as a line: where, as far as the computer says, and what. */
@Composable
private fun mistakeLine(m: Mistake): String = when {
	m.file != null && m.line != null -> stringResource(R.string.editor_mistake_line, m.file, m.line, m.message)
	m.path != null -> stringResource(R.string.editor_mistake_path, m.path, m.message)
	else -> m.message
}

/** What is said over what is edited: the file can't be written, another editor wrote meanwhile, mistakes elsewhere. */
@Composable
private fun EditorNotes(state: EditorState, desc: EditorDescription, elsewhere: List<Mistake>, onTakeTheirs: () -> Unit, onSaveMine: () -> Unit) {
	if (!desc.writable) {
		Banner(stringResource(R.string.editor_not_writable, desc.why ?: desc.file))
	}
	state.conflict?.let { c ->
		Banner(stringResource(if (c.mine) R.string.editor_conflict_mine else R.string.editor_conflict_other)) {
			Row(horizontalArrangement = Arrangement.spacedBy(Space.s)) {
				// Only where this itself changed is there something of the computer's to take.
				if (c.mine) OutlinedButton(onClick = onTakeTheirs) { Text(stringResource(R.string.editor_conflict_take)) }
				Button(onClick = onSaveMine, enabled = !state.busy && desc.writable) { Text(stringResource(R.string.editor_conflict_keep)) }
			}
		}
	}
	if (elsewhere.isNotEmpty()) {
		Banner(stringResource(R.string.editor_mistakes)) {
			for (m in elsewhere) Text(mistakeLine(m), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.error)
		}
	}
}

/** A note in a card the warning's colour, with what may be done about it. */
@Composable
private fun Banner(text: String, more: (@Composable () -> Unit)? = null) {
	val warn = Metric.colors.warn
	Surface(shape = RoundedCornerShape(Radius.lPlus), color = warn.copy(alpha = Alpha.BANNER), modifier = Modifier.fillMaxWidth()) {
		Column(Modifier.padding(Space.l), verticalArrangement = Arrangement.spacedBy(Space.s)) {
			Row(verticalAlignment = Alignment.Top) {
				Icon(Icons.Default.Warning, null, tint = warn, modifier = Modifier.size(Sizes.iconM))
				Text(text, style = MaterialTheme.typography.bodyMedium, modifier = Modifier.padding(start = Space.s))
			}
			more?.invoke()
		}
	}
}

/** Padding for the editor's content under the glass bar, with room for the button at the bottom. */
@Composable
private fun editorPadding(): PaddingValues {
	val d = LocalDensity.current
	val top = with(d) { WindowInsets.statusBars.getTop(d).toDp() } + topBarHeight() + Space.s
	val bottom = with(d) { WindowInsets.navigationBars.getBottom(d).toDp() } + FAB_ROOM
	return PaddingValues(start = Space.l, end = Space.l, top = top, bottom = bottom)
}

/** A widget being dragged over the editor's grid: which, how far, and the place it would take. */
private class Drag {
	var from by mutableStateOf<Int?>(null)
		private set
	var offset by mutableStateOf(Offset.Zero)
	var target by mutableStateOf<Int?>(null)
		private set

	/** Where the finger is, in the window's terms as the cells' bounds. */
	var pointer = Offset.Zero
		private set

	/** Where the dragged widget was when it was lifted, moved with the list as it scrolls. */
	var home = Rect.Zero

	/** Each widget's place as last laid out, by its place in the dashboard. */
	val bounds = HashMap<Int, Rect>()

	/** What of the list can be seen. */
	var viewport = Rect.Zero

	fun start(i: Int, at: Offset) {
		home = bounds[i] ?: Rect.Zero
		pointer = at
		offset = Offset.Zero
		target = null
		from = i
	}

	fun by(delta: Offset) {
		offset += delta
		pointer += delta
		aim()
	}

	/** The place under the finger: another widget's, nothing over its own, else the one before. */
	fun aim() {
		val f = from ?: return
		if (home.contains(pointer)) {
			target = null
			return
		}
		bounds.entries.firstOrNull { (i, r) -> i != f && r.contains(pointer) }?.let { target = it.key }
	}

	/** Dropped: where from and to, when it moved. */
	fun end(): Pair<Int, Int>? {
		val f = from
		val t = target
		from = null
		target = null
		offset = Offset.Zero
		return if (f != null && t != null && f != t) f to t else null
	}
}

/** The dashboard being edited: notes, then its widgets as the computer's preview draws them, each with a handle and a sheet. */
@Composable
private fun BoardEditor(
	state: EditorState,
	desc: EditorDescription,
	board: Editing.Board,
	env: FormEnv,
	machineVm: MachineViewModel,
	machineState: MachineState,
	machineId: String,
	name: String,
	lower: List<EditorFrom>,
	/** The preview's graphs' and sparks' points, by widget id. */
	hist: (String) -> Hist?,
	onOpen: (List<Int>) -> Unit,
	onMove: (Int, Int) -> Unit,
	onTakeTheirs: () -> Unit,
	onSaveMine: () -> Unit,
) {
	val ctx = LocalContext.current
	val scroll = rememberScrollState()
	val drag = remember { Drag() }
	val mistakes = BoardMistakes(state.allMistakes, board.id)
	val view = state.preview
	val ui = view?.preview?.ui
	val line = view?.preview?.line
	val shownDash: Dashboard? = ui?.let { u -> u.dashboard(line?.dashboard ?: board.page) ?: u.dashboards.firstOrNull { it.id == board.id || it.id.startsWith(board.id + ".") } }
	val folds = remember(machineId) { Folds(Settings.get(ctx), machineId) }
	val tables = remember { TableStates() }
	val form = remember { mutableStateOf<FormRequest?>(null) }
	val res = resources()
	val scope = if (ui != null && shownDash != null) {
		// Drawn as on the computer's screen, but nothing in it runs: every touch opens the widget's sheet instead.
		val launcher = ActionLauncher(LocalGuard.current, { _, _, _, _ -> }, ui, name, res, tap = {}, form = form, running = ActionsState())
		val shownState = MachineState(conn = Conn.Live, host = machineState.host, ui = ui, data = line?.let { mapOf(shownDash.id to it) }.orEmpty(), receivedAt = view.receivedAt, logs = view.logs)
		DashScope(machineId, ui, shownDash.id, line?.widgets.orEmpty(), machineVm, shownState, name, open = {}, actions = launcher, folds = folds, tables = tables, sorts = emptyMap(), logs = emptyMap(), hist = hist)
	} else null
	val haptics = LocalHapticFeedback.current
	val prefs = LocalPrefs.current
	val density = LocalDensity.current
	val edgePx = with(density) { EDGE.toPx() }
	// What the bar covers at the top: the list scrolls up from under it.
	val coveredPx = with(density) { WindowInsets.statusBars.getTop(density) + topBarHeight().toPx() }
	val stepPx = with(LocalDensity.current) { STEP.toPx() }

	// Near the top or bottom while dragging, the list scrolls under the finger, the widget with it.
	LaunchedEffect(drag.from) {
		if (drag.from == null) return@LaunchedEffect
		while (isActive && drag.from != null) {
			val y = drag.pointer.y
			val v = drag.viewport
			val step = when {
				y < v.top + coveredPx + edgePx -> -stepPx
				y > v.bottom - edgePx -> stepPx
				else -> 0f
			}
			if (step != 0f) {
				val used = scroll.scrollBy(step)
				if (used != 0f) {
					drag.offset += Offset(0f, used)
					drag.home = drag.home.translate(0f, -used)
					drag.aim()
				}
			}
			withFrameNanos {}
		}
	}

	Column(
		Modifier
			.fillMaxSize()
			.onGloballyPositioned { drag.viewport = it.boundsInRoot() }
			.verticalScroll(scroll)
			.padding(editorPadding()),
		verticalArrangement = Arrangement.spacedBy(Space.m),
	) {
		EditorNotes(state, desc, mistakes.elsewhere, onTakeTheirs, onSaveMine)
		Mistakes(mistakes.ofKeys.values.flatten())
		state.previewNote?.let { Text(it.text(res), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant) }
		val widgets = Drafts.widgets(state.draft)
		if (widgets.isEmpty()) {
			Text(stringResource(R.string.editor_empty), style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
		}
		val objects = widgets.map { it as? JsonObject ?: JsonObject(emptyMap()) }
		val shown = previewOf(widgets, shownDash)
		val columns = gridColumns()
		// One spread over a collection's items takes the whole width, its items in a grid of their own.
		val spans = objects.mapIndexed { i, w -> if (shown[i].size > 1) Grid.COLUMNS else desc.spanOf(w) }
		for (row in Grid.place(spans, columns)) {
			val lifted = row.any { it.first == drag.from }
			Row(Modifier.fillMaxWidth().zIndex(if (lifted) 1f else 0f), horizontalArrangement = Arrangement.spacedBy(Space.m)) {
				for ((i, span) in row) {
					EditCell(
						i, objects[i], shown[i], scope, desc, columns, span, drag, mistakes.ofCell(i),
						Modifier.weight(span.toFloat()),
						onOpen = { onOpen(listOf(i)) },
						onMoveBy = { by -> if (i + by in widgets.indices) onMove(i, i + by) },
						onLift = { if (prefs.haptics) haptics.performHapticFeedback(HapticFeedbackType.LongPress) },
						onDrop = { drag.end()?.let { (f, t) -> onMove(f, t) } },
					)
				}
				val used = row.sumOf { it.second }
				if (used < columns) Spacer(Modifier.weight((columns - used).toFloat()))
			}
		}
	}
}

/**
 * One widget of the draft: what the preview drew of it (nothing where it isn't shown now: a card
 * that says so), the computer's mistakes under it, a tap to its sheet, and a handle that lifts it
 * with a long press to drag it to another widget's place.
 */
@Composable
private fun EditCell(
	i: Int,
	w: JsonObject,
	shown: List<Widget>,
	scope: DashScope?,
	desc: EditorDescription,
	columns: Int,
	span: Int,
	drag: Drag,
	mistakes: List<String>,
	modifier: Modifier,
	onOpen: () -> Unit,
	onMoveBy: (Int) -> Unit,
	onLift: () -> Unit,
	onDrop: () -> Unit,
) {
	val lifted = drag.from == i
	val mark = when {
		mistakes.isNotEmpty() -> MaterialTheme.colorScheme.error
		drag.target == i -> MaterialTheme.colorScheme.primary
		else -> null
	}
	val label = listOfNotNull(desc.typeLabel(w), w["label"].text()?.ifBlank { null }).joinToString(" · ")
	val shadow = with(LocalDensity.current) { LIFT_SHADOW.toPx() }
	Box(
		modifier
			.onGloballyPositioned { if (drag.from != i) drag.bounds[i] = it.boundsInRoot() }
			.graphicsLayer {
				if (lifted) {
					translationX = drag.offset.x
					translationY = drag.offset.y
					scaleX = LIFT
					scaleY = LIFT
					alpha = LIFTED_ALPHA
					shadowElevation = shadow
					shape = CardShape
				}
			}
			.then(if (mark != null) Modifier.border(MARK, mark, CardShape) else Modifier),
	) {
		Column(verticalArrangement = Arrangement.spacedBy(Space.xs)) {
			when {
				scope == null || shown.isEmpty() -> Hidden(w, desc)
				shown.size == 1 -> WidgetView(scope, shown[0], Modifier.fillMaxWidth(), columns)
				else -> {
					val inner = Grid.span(span, columns)
					Column(verticalArrangement = Arrangement.spacedBy(Space.s)) {
						for (r in Grid.place(shown.map { it.span }, inner)) GridRow(scope, r.map { (j, s) -> shown[j] to s }, inner)
					}
				}
			}
			if (mistakes.isNotEmpty()) Column(Modifier.padding(horizontal = Space.s)) { Mistakes(mistakes) }
		}
		// Over the widget: a touch edits it, and runs nothing of it.
		Box(
			Modifier
				.matchParentSize()
				.clip(CardShape)
				.clickable(role = Role.Button, onClickLabel = stringResource(R.string.editor_edit_widget)) { onOpen() }
				.semantics { contentDescription = label },
		)
		DragHandle(i, drag, Modifier.align(Alignment.TopEnd), onMoveBy, onLift, onDrop)
	}
}

/** The handle a widget is dragged by, after a long press: a swipe that starts on it still scrolls the list. */
@Composable
private fun DragHandle(i: Int, drag: Drag, modifier: Modifier, onMoveBy: (Int) -> Unit, onLift: () -> Unit, onDrop: () -> Unit) {
	var at by remember { mutableStateOf(Offset.Zero) }
	// The gesture outlives a recomposition: it calls what is current then.
	val lift by rememberUpdatedState(onLift)
	val drop by rememberUpdatedState(onDrop)
	val earlier = stringResource(R.string.editor_move_earlier)
	val later = stringResource(R.string.editor_move_later)
	val description = stringResource(R.string.editor_drag)
	Box(
		modifier
			.padding(Space.xs)
			.size(Sizes.touchTarget)
			.onGloballyPositioned { at = it.positionInRoot() }
			.clip(CircleShape)
			.background(MaterialTheme.colorScheme.surfaceContainerHighest.copy(alpha = HANDLE_ALPHA))
			.pointerInput(i) {
				detectDragGesturesAfterLongPress(
					onDragStart = { p ->
						drag.start(i, at + p)
						lift()
					},
					onDrag = { change, by ->
						change.consume()
						drag.by(by)
					},
					onDragEnd = { drop() },
					onDragCancel = { drop() },
				)
			}
			.semantics {
				contentDescription = description
				// Without dragging: a screen reader's actions, the sheet's buttons.
				customActions = listOf(
					CustomAccessibilityAction(earlier) { onMoveBy(-1); true },
					CustomAccessibilityAction(later) { onMoveBy(1); true },
				)
			},
		contentAlignment = Alignment.Center,
	) {
		Icon(Icons.Default.Menu, null, tint = MaterialTheme.colorScheme.onSurfaceVariant, modifier = Modifier.size(Sizes.iconL))
	}
}

private const val HANDLE_ALPHA = 0.85f

/** A widget the preview doesn't show now (its `when`, the role, a value not there yet): a card that says so. */
@Composable
private fun Hidden(w: JsonObject, desc: EditorDescription) {
	val typeLabel = desc.typeLabel(w)
	Section(w["label"].text()?.ifBlank { null } ?: typeLabel, icon(w["icon"].text()), accent = MaterialTheme.colorScheme.secondary) {
		Text(stringResource(R.string.editor_not_shown, typeLabel), style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
		// Room for the handle beside the title.
		Spacer(Modifier.height(Space.xs))
	}
}

/** The widget at [path]: moved within its list, its keys, its width among them, taken out; a group's children opened in its place. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun WidgetSheet(
	path: List<Int>,
	state: EditorState,
	desc: EditorDescription,
	board: String,
	env: FormEnv,
	onKey: (String, JsonElement?) -> Unit,
	onMove: (Int) -> Unit,
	onRemove: () -> Unit,
	onOpen: (List<Int>?) -> Unit,
) {
	val w = Drafts.widget(state.draft, path)
	if (w == null) {
		LaunchedEffect(path) { onOpen(null) }
		return
	}
	val siblings = Drafts.widget(state.draft, path.dropLast(1))?.let(Drafts::widgets)?.size ?: 0
	val i = path.last()
	val mistakes = BoardMistakes(state.allMistakes, board).ofWidget(path)
	val sheet = rememberModalBottomSheetState(skipPartiallyExpanded = true)
	ModalBottomSheet(onDismissRequest = { onOpen(null) }, sheetState = sheet, containerColor = MaterialTheme.colorScheme.surfaceContainerLow) {
		Column(
			Modifier.fillMaxWidth().verticalScroll(rememberScrollState()).imePadding().padding(start = Space.xl, end = Space.xl, bottom = Space.xxl),
			verticalArrangement = Arrangement.spacedBy(Space.l),
		) {
			if (path.size > 1) {
				val parent = Drafts.widget(state.draft, path.dropLast(1))
				TextButton(onClick = { onOpen(path.dropLast(1)) }) {
					Icon(Icons.AutoMirrored.Filled.ArrowBack, null, Modifier.size(Sizes.iconM))
					Text(stringResource(R.string.editor_back_to, parent?.let { listOfNotNull(desc.typeLabel(it), it["label"].text()).joinToString(" · ") }.orEmpty()), Modifier.padding(start = Space.xs))
				}
			}
			Text(listOfNotNull(desc.typeLabel(w), w["label"].text()?.ifBlank { null }).joinToString(" · "), style = MaterialTheme.typography.titleLarge)
			Row(horizontalArrangement = Arrangement.spacedBy(Space.s)) {
				OutlinedButton(onClick = { onMove(-1) }, enabled = i > 0) {
					Icon(Icons.Default.KeyboardArrowUp, null, Modifier.size(Sizes.iconM))
					Text(stringResource(R.string.editor_move_earlier), Modifier.padding(start = Space.xs))
				}
				OutlinedButton(onClick = { onMove(1) }, enabled = i < siblings - 1) {
					Icon(Icons.Default.KeyboardArrowDown, null, Modifier.size(Sizes.iconM))
					Text(stringResource(R.string.editor_move_later), Modifier.padding(start = Space.xs))
				}
			}
			Mistakes(mistakes[""].orEmpty())
			// Its type is its title; changed only where the description offers it.
			ObjectFields(desc.keysOf(w), w, onKey, env, path, mistakes, skip = if (desc.common.none { it.key == TYPE }) setOf(TYPE) else emptySet())
			TextButton(onClick = onRemove) {
				Icon(Icons.Default.Delete, null, Modifier.size(Sizes.iconM), tint = MaterialTheme.colorScheme.error)
				Text(stringResource(R.string.editor_remove_widget), Modifier.padding(start = Space.xs), color = MaterialTheme.colorScheme.error)
			}
		}
	}
}

/** The dashboard's own keys (its title, icon, role…) as the description has them, and where it comes from. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun DashboardKeysSheet(
	state: EditorState,
	desc: EditorDescription,
	board: String,
	env: FormEnv,
	lower: List<EditorFrom>,
	onKey: (String, JsonElement?) -> Unit,
	onDismiss: () -> Unit,
) {
	val sheet = rememberModalBottomSheetState(skipPartiallyExpanded = true)
	val mistakes = BoardMistakes(state.allMistakes, board).ofKeys
	val known = desc.dashboard(board)
	ModalBottomSheet(onDismissRequest = onDismiss, sheetState = sheet, containerColor = MaterialTheme.colorScheme.surfaceContainerLow) {
		Column(
			Modifier.fillMaxWidth().verticalScroll(rememberScrollState()).imePadding().padding(start = Space.xl, end = Space.xl, bottom = Space.xxl),
			verticalArrangement = Arrangement.spacedBy(Space.l),
		) {
			Text(stringResource(R.string.editor_dashboard_keys), style = MaterialTheme.typography.titleLarge)
			Text(stringResource(R.string.editor_dashboard_id, board), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
			known?.from?.takeIf { it.isNotEmpty() }?.let { from ->
				Text(
					stringResource(R.string.editor_set_in, from.joinToString(", ") { f -> f.line?.let { l -> "${f.file}:$l" } ?: f.file }),
					style = MaterialTheme.typography.bodySmall,
					color = MaterialTheme.colorScheme.onSurfaceVariant,
				)
			}
			// The widgets are the grid's to edit.
			ObjectFields(desc.dashboard.keys, state.draft, onKey, env, emptyList(), mistakes, fixedBy(lower, state.base), skip = setOf(WIDGETS))
		}
	}
}

/** The tabs of a profile: whose, which dashboards in which order, the header, and the dashboards of the phone's widget, tiles and card. */
@Composable
private fun TabsEditor(
	state: EditorState,
	desc: EditorDescription,
	tabs: Editing.Tabs,
	env: FormEnv,
	lower: List<EditorFrom>,
	onKey: (String, JsonElement?) -> Unit,
	onAndroid: (String, JsonElement?) -> Unit,
	onProfile: (String) -> Unit,
	onTakeTheirs: () -> Unit,
	onSaveMine: () -> Unit,
) {
	val profile = desc.profile(tabs.profile)
	val placed = state.allMistakes.map { it to mistakeAt(it.path) }
	val ofKey = placed.mapNotNull { (m, a) -> (a as? MistakeAt.InProfile)?.takeIf { it.profile == tabs.profile }?.let { (it.key ?: "") to m.message } }.groupBy({ it.first }, { it.second })
	val elsewhere = placed.filter { (_, a) -> (a as? MistakeAt.InProfile)?.profile != tabs.profile }.map { it.first }
	val fixed = fixedBy(lower, state.base)
	val android = state.draft[ANDROID] as? JsonObject ?: JsonObject(emptyMap())
	val baseAndroid = state.base[ANDROID] as? JsonObject ?: JsonObject(emptyMap())
	val fixedAndroid = fixedBy(lower, baseAndroid)
	var picking by rememberSaveable { mutableStateOf(false) }
	val dashboardRef = { key: String, label: String, hint: String? -> EditorKey(key = key, kind = "ref", to = "dashboard", label = label, hint = hint) }
	Column(
		Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(editorPadding()),
		verticalArrangement = Arrangement.spacedBy(Space.l),
	) {
		EditorNotes(state, desc, elsewhere, onTakeTheirs, onSaveMine)
		Mistakes(ofKey[""].orEmpty())
		Column(verticalArrangement = Arrangement.spacedBy(Space.xs)) {
			Text(stringResource(R.string.editor_profile), style = MaterialTheme.typography.labelLarge, color = MaterialTheme.colorScheme.primary)
			val own = profile?.id == desc.profile
			val shownName = profile?.let { p -> p.name.ifBlank { p.id } }.orEmpty()
			PickerField(if (own) stringResource(R.string.editor_profile_own, shownName) else shownName) { if (!state.dirty) picking = true }
			if (state.dirty) {
				Text(stringResource(R.string.editor_profile_dirty), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
			} else profile?.let { p ->
				Text(pluralStringResource(R.plurals.editor_profile_clients, p.clients.size, p.clients.size), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
			}
		}
		val list = EditorKey(key = "dashboards", kind = "list", label = stringResource(R.string.editor_tabs), of = buildJsonObject {
			put("kind", JsonPrimitive("ref"))
			put("to", JsonPrimitive("dashboard"))
		})
		KeyField(list, state.draft["dashboards"], { onKey("dashboards", it) }, env, ofKey["dashboards"].orEmpty(), fixed("dashboards"))
		KeyField(dashboardRef("header", stringResource(R.string.editor_header), stringResource(R.string.editor_header_hint)), state.draft["header"], { onKey("header", it) }, env, ofKey["header"].orEmpty(), fixed("header"))
		Text(stringResource(R.string.editor_android), style = MaterialTheme.typography.titleMedium)
		for ((key, label) in listOf(WIDGET to R.string.editor_android_widget, TILES to R.string.editor_android_tiles, CARD to R.string.editor_android_card)) {
			KeyField(dashboardRef(key, stringResource(label), null), android[key], { onAndroid(key, it) }, env, ofKey["$ANDROID.$key"].orEmpty(), fixedAndroid(key))
		}
		// What else the profile's table holds, as the computer has it: kept, as JSON.
		for (k in state.draft.keys - setOf("dashboards", "header", ANDROID)) {
			KeyField(EditorKey(key = k), state.draft[k], { onKey(k, it) }, env, ofKey[k].orEmpty(), fixed(k))
		}
		for (k in android.keys - setOf(WIDGET, TILES, CARD)) {
			KeyField(EditorKey(key = "$ANDROID.$k"), android[k], { onAndroid(k, it) }, env, ofKey["$ANDROID.$k"].orEmpty(), fixedAndroid(k))
		}
	}
	if (picking) {
		PickDialog(
			stringResource(R.string.editor_profile),
			desc.profiles.map { p -> PickItem(p.id, p.name.ifBlank { p.id }, p.id) },
			onPick = onProfile,
			onDismiss = { picking = false },
		)
	}
}

private const val WIDGET = "widget"
private const val TILES = "tiles"
private const val CARD = "card"

/** A new dashboard: its id, its title, empty or a copy of another, and whether this phone's tabs get it. */
@Composable
private fun NewDashboardDialog(desc: EditorDescription, onDismiss: () -> Unit, onCreate: (String, String, String?, Boolean) -> Unit) {
	var id by rememberSaveable { mutableStateOf("") }
	var title by rememberSaveable { mutableStateOf("") }
	var copy by rememberSaveable { mutableStateOf<String?>(null) }
	var toTabs by rememberSaveable { mutableStateOf(true) }
	var picking by rememberSaveable { mutableStateOf(false) }
	val taken = desc.dashboard(id) != null
	val valid = isSettingsId(id) && !taken && title.isNotBlank()
	GlassDialog(
		onDismiss = onDismiss,
		title = stringResource(R.string.editor_new_title),
		text = {
			Column(verticalArrangement = Arrangement.spacedBy(Space.m)) {
				OutlinedTextField(
					value = title,
					onValueChange = { title = it },
					label = { Text(stringResource(R.string.editor_new_name)) },
					singleLine = true,
					modifier = Modifier.fillMaxWidth(),
				)
				OutlinedTextField(
					value = id,
					onValueChange = { id = it.lowercase() },
					label = { Text(stringResource(R.string.editor_new_id)) },
					supportingText = { Text(stringResource(if (taken) R.string.editor_new_id_taken else R.string.editor_new_id_rule)) },
					isError = taken || (id.isNotEmpty() && !isSettingsId(id)),
					singleLine = true,
					keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Ascii, capitalization = KeyboardCapitalization.None, autoCorrectEnabled = false),
					modifier = Modifier.fillMaxWidth(),
				)
				Text(stringResource(R.string.editor_new_from), style = MaterialTheme.typography.labelLarge, color = MaterialTheme.colorScheme.primary)
				PickerField(copy?.let { c -> desc.dashboard(c)?.title?.ifBlank { null } ?: c } ?: stringResource(R.string.editor_new_empty)) { picking = true }
				if (desc.profile != null) {
					Row(
						Modifier.fillMaxWidth().clickable(role = Role.Checkbox) { toTabs = !toTabs },
						verticalAlignment = Alignment.CenterVertically,
					) {
						Checkbox(checked = toTabs, onCheckedChange = null)
						Text(stringResource(R.string.editor_new_tabs), Modifier.padding(start = Space.s))
					}
				}
			}
		},
		confirm = { TextButton(onClick = { onCreate(id, title.trim(), copy, toTabs) }, enabled = valid) { Text(stringResource(R.string.editor_new_create)) } },
		dismiss = { TextButton(onClick = onDismiss) { Text(stringResource(R.string.cancel)) } },
	)
	if (picking) {
		val empty = stringResource(R.string.editor_new_empty)
		PickDialog(
			stringResource(R.string.editor_new_from),
			listOf(PickItem("", empty)) + desc.dashboards.map { PickItem(it.id, it.title.ifBlank { it.id }, it.id) },
			onPick = { copy = it.ifEmpty { null } },
			onDismiss = { picking = false },
		)
	}
}
