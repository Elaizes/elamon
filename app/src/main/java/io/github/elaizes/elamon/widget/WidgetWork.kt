package io.github.elaizes.elamon.widget

import android.appwidget.AppWidgetManager
import android.content.ComponentName
import android.content.Context
import androidx.core.content.edit
import androidx.glance.appwidget.updateAll
import androidx.work.Constraints
import androidx.work.CoroutineWorker
import androidx.work.ExistingPeriodicWorkPolicy
import androidx.work.ExistingWorkPolicy
import androidx.work.NetworkType
import androidx.work.OneTimeWorkRequestBuilder
import androidx.work.PeriodicWorkRequestBuilder
import androidx.work.WorkManager
import androidx.work.WorkerParameters
import androidx.work.workDataOf
import java.util.concurrent.TimeUnit
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.launch
import io.github.elaizes.elamon.Outside
import io.github.elaizes.elamon.tiles.refreshTiles
import io.github.elaizes.elamon.Reach
import io.github.elaizes.elamon.Settings
import io.github.elaizes.elamon.data.ElamondException
import io.github.elaizes.elamon.data.Machine
import io.github.elaizes.elamon.data.MachineStore
import io.github.elaizes.elamon.data.UiCache
import io.github.elaizes.elamon.reach
import io.github.elaizes.elamon.readDashboard
import io.github.elaizes.elamon.uiOf

/**
 * Reads the computer [KEY_MACHINE] for its home-screen widgets: its `ui` as kept (the computer's own
 * where that is out of date, names no dashboard for the widget, or [KEY_FRESH] asks for it), and
 * one snapshot of the profile's `widget` without history. What it finds is kept for the widgets,
 * which are drawn anew. Run by WorkManager every `widget_minutes` and on a tap; no connection is kept.
 */
class WidgetWorker(context: Context, params: WorkerParameters) : CoroutineWorker(context, params) {
	override suspend fun doWork(): Result {
		val ctx = applicationContext
		val id = inputData.getString(KEY_MACHINE) ?: return Result.success()
		val machine = MachineStore.get(ctx).get(id) ?: return Result.success()
		val widgets = WidgetStore.get(ctx)
		widgets.reading(id, true)
		ElamonWidget().updateAll(ctx)
		try {
			val shown = read(ctx, machine, widgets.get(id), fresh = inputData.getBoolean(KEY_FRESH, false))
			// Forgotten while it was read: nothing of it is kept.
			if (MachineStore.get(ctx).get(id) != null) widgets.put(id, shown)
		} finally {
			widgets.reading(id, false)
		}
		ElamonWidget().updateAll(ctx)
		// The profile may ask for another pace since.
		WidgetWork.sync(ctx)
		return Result.success()
	}

	private suspend fun read(ctx: Context, machine: Machine, had: WidgetShown, fresh: Boolean): WidgetShown {
		val store = MachineStore.get(ctx)
		val cache = UiCache.get(ctx)
		// Taken before asking: forgotten meanwhile, the computer's answer isn't kept.
		val ticket = cache.ticket(machine.id)
		return try {
			val client = when (val r = reach(machine, store)) {
				is Reach.Reached -> r.client
				Reach.Offline -> return had.tried(Fetch.Offline)
				Reach.Unpaired -> return had.tried(Fetch.Unpaired)
				Reach.Outdated -> return had.tried(Fetch.Outdated)
			}
			val (ui, line) = readDashboard(
				ui = { again -> uiOf(machine, client, cache, store, fresh || again, ticket) },
				pick = ::widgetDashboard,
				snapshot = { client.snapshot(it, history = false) },
				freshWithout = !fresh,
			)
			val profile = ui.profile
			val base = had.copy(profile = profile.id, minutes = widgetMinutes(profile.android.widgetMinutes))
			val dashboard = widgetDashboard(ui)
			if (line == null || dashboard == null) return base.copy(fetch = Fetch.NoDashboard, lines = emptyList(), readAt = 0, title = null, triedAt = System.currentTimeMillis())
			base.copy(
				lines = widgetLines(ui, dashboard, line.widgets),
				readAt = System.currentTimeMillis(),
				triedAt = System.currentTimeMillis(),
				fetch = Fetch.Ok,
				title = ui.dashboard(dashboard)?.title,
			)
		} catch (e: CancellationException) {
			throw e
		} catch (e: Exception) {
			had.tried(
				when ((e as? ElamondException)?.kind) {
					ElamondException.Kind.Unauthorized -> Fetch.Unpaired
					null, ElamondException.Kind.Unreachable, ElamondException.Kind.CertificateChanged -> Fetch.Offline
					else -> Fetch.Failed
				},
			)
		}
	}

	companion object {
		const val KEY_MACHINE = "machine"
		const val KEY_FRESH = "fresh"
	}
}

/**
 * The widgets' work with WorkManager: a periodic reading of each computer some widget shows, at the
 * pace its profile asks (Android allows no less than 15 minutes), and one at once on a tap. It is
 * kept across restarts of the phone, and runs once the phone has been unlocked after one.
 */
object WidgetWork {
	private const val PERIODIC = "widget/"
	private const val NOW = "widget-now/"
	private const val PREFS = "widget_work"

	/** The computers the home-screen widgets show now. */
	fun shownMachines(context: Context): Set<String> {
		val ids = widgetIds(context)
		if (ids.isEmpty()) return emptySet()
		val paired = MachineStore.get(context).machines.value
		val settings = Settings.get(context)
		return ids.toList().mapNotNull { shownMachine(settings.widgetMachine(it), paired)?.id }.toSet()
	}

	fun widgetIds(context: Context): IntArray =
		AppWidgetManager.getInstance(context)?.getAppWidgetIds(ComponentName(context, ElamonWidgetReceiver::class.java)) ?: IntArray(0)

	/**
	 * The periodic readings as the widgets need them: one for each computer shown, at its profile's
	 * pace, put in place again only where that changed; none for a computer no widget shows.
	 */
	@Synchronized
	fun sync(context: Context) {
		val ctx = context.applicationContext
		val wm = WorkManager.getInstance(ctx)
		val prefs = ctx.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
		val widgets = WidgetStore.get(ctx)
		val wanted = shownMachines(ctx).associateWith { widgets.get(it).minutes }
		val had = prefs.all.mapNotNull { (k, v) -> (v as? Int)?.let { k to it } }.toMap()
		prefs.edit {
			for ((id, minutes) in wanted) {
				if (had[id] == minutes) continue
				val request = PeriodicWorkRequestBuilder<WidgetWorker>(minutes.toLong(), TimeUnit.MINUTES)
					.setConstraints(Constraints.Builder().setRequiredNetworkType(NetworkType.CONNECTED).build())
					.setInputData(workDataOf(WidgetWorker.KEY_MACHINE to id))
					.build()
				wm.enqueueUniquePeriodicWork(PERIODIC + id, ExistingPeriodicWorkPolicy.UPDATE, request)
				putInt(id, minutes)
			}
			for (id in had.keys - wanted.keys) {
				wm.cancelUniqueWork(PERIODIC + id)
				remove(id)
			}
		}
	}

	/**
	 * Reads [machine] for its widgets at once, its `ui` fresh from it: a tap, or a widget just added;
	 * a reading already on its way is started again with [policy] REPLACE, left to go on with KEEP.
	 */
	fun now(context: Context, machine: String, policy: ExistingWorkPolicy = ExistingWorkPolicy.REPLACE) {
		val request = OneTimeWorkRequestBuilder<WidgetWorker>()
			.setInputData(workDataOf(WidgetWorker.KEY_MACHINE to machine, WidgetWorker.KEY_FRESH to true))
			.build()
		WorkManager.getInstance(context.applicationContext).enqueueUniqueWork(NOW + machine, policy, request)
	}

	/**
	 * The widgets' computers that haven't been read within their pace (or ever): read at once, as
	 * when a widget is added or the phone has restarted.
	 */
	fun catchUp(context: Context, now: Long = System.currentTimeMillis()) {
		val widgets = WidgetStore.get(context)
		for (id in shownMachines(context)) {
			val s = widgets.get(id)
			// One on its way already (a tap, a widget just set) is left to finish.
			if (stale(s, now)) now(context, id, ExistingWorkPolicy.KEEP)
		}
	}

	/** Whether [s] was last tried longer ago than its pace, or never: read or not, so a computer out of reach isn't asked at every turn. */
	fun stale(s: WidgetShown, now: Long): Boolean {
		val last = maxOf(s.readAt, s.triedAt)
		return last == 0L || now - last >= TimeUnit.MINUTES.toMillis(s.minutes.toLong())
	}
}

/**
 * A computer paired or forgotten: the widgets follow (one that showed a computer forgotten shows the
 * first paired), their work with them, and a computer newly shown is read at once; and the tiles.
 */
fun machinesChanged(context: Context) {
	val ctx = context.applicationContext
	refreshTiles(ctx)
	WidgetWork.sync(ctx)
	WidgetWork.catchUp(ctx)
	Outside.scope.launch { runCatching { ElamonWidget().updateAll(ctx) } }
}
