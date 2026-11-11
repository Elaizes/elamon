package io.github.elaizes.elamon

import android.app.Application
import android.net.Uri
import android.provider.DocumentsContract
import androidx.annotation.StringRes
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import io.github.elaizes.elamon.data.ImportConflict
import io.github.elaizes.elamon.data.KeystoreException
import io.github.elaizes.elamon.data.Machine
import io.github.elaizes.elamon.data.MachineStore
import io.github.elaizes.elamon.data.PairingsFile
import io.github.elaizes.elamon.data.importConflicts
import java.io.ByteArrayOutputStream
import java.io.IOException
import java.io.InputStream
import java.time.LocalDate
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.ensureActive
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.job
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

/** Where exporting or importing pairings is, for Settings to show. */
sealed interface Transfer {
	data object Idle : Transfer

	/** The file to write was chosen, but the passphrase was lost meanwhile: the app was closed while the file was picked. */
	data class ExportNeedsPassphrase(val uri: Uri) : Transfer

	/** A file of pairings was read; its passphrase is next. [wrong] after one that didn't open it. */
	data class ImportNeedsPassphrase(val wrong: Boolean = false) : Transfer

	/** Making the key and sealing or opening (a second or two, which [cancellable] lets the person give up on), or saving. */
	data class Working(val export: Boolean, val cancellable: Boolean = true) : Transfer

	/**
	 * The file would put [conflict]'s computer in place of the one here, which differs from it:
	 * replace it, or keep this phone's? The [index]th such of [count], from 0.
	 */
	data class ImportConflictAsked(val conflict: ImportConflict, val index: Int, val count: Int) : Transfer

	data class Exported(val count: Int, val leftOut: Int) : Transfer

	/** [kept]: computers the file had that were left as they are here, as the person chose. */
	data class Imported(val added: Int, val replaced: Int, val kept: Int = 0) : Transfer

	/** [export] or an import failed: [message] says why. */
	data class Failed(@StringRes val message: Int, val export: Boolean) : Transfer
}

/**
 * Settings → Export pairings and Import pairings. The passphrase and the file read are held here,
 * in memory only, between the dialog and the system's file picker, and dropped once used; nothing
 * of them is saved, not even across the app being closed.
 */
class TransferViewModel(app: Application) : AndroidViewModel(app) {
	private val _state = MutableStateFlow<Transfer>(Transfer.Idle)
	val state: StateFlow<Transfer> = _state.asStateFlow()

	private val store get() = MachineStore.get(getApplication())
	private val resolver get() = getApplication<Application>().contentResolver

	/** The passphrase chosen for an export, while the file to write is picked. */
	private var exportPassphrase: CharArray? = null

	/** The file being imported, read whole and still sealed, while its passphrase is asked. */
	private var importBytes: ByteArray? = null

	/** The file's computers, opened, with their tokens, while the person is asked about those that differ from the ones here. */
	private var opened: List<Machine>? = null
	private var conflicts: List<ImportConflict> = emptyList()

	/** Of [conflicts], the ids of the computers left as they are here. */
	private val kept = mutableSetOf<String>()

	/** Making the key, which the person can give up on. */
	private var work: Job? = null

	/** Keeps [passphrase] for the export about to have its file picked. */
	fun exportPassphrase(passphrase: CharArray) {
		exportPassphrase?.fill('\u0000')
		exportPassphrase = passphrase
	}

	/** The file picked to export to, or null when picking was cancelled. */
	fun exportTo(uri: Uri?) {
		val passphrase = exportPassphrase
		exportPassphrase = null
		when {
			uri == null -> passphrase?.fill('\u0000')
			passphrase == null -> _state.value = Transfer.ExportNeedsPassphrase(uri)
			else -> export(uri, passphrase)
		}
	}

	/** The passphrase asked again for [uri], after the app was closed while it was picked. */
	fun export(uri: Uri, passphrase: CharArray) {
		_state.value = Transfer.Working(export = true)
		work = viewModelScope.launch {
			try {
				_state.value = withContext(Dispatchers.Default) {
					val (machines, leftOut) = store.withTokens()
					if (machines.isEmpty()) {
						forget(uri)
						return@withContext Transfer.Failed(R.string.transfer_nothing_to_export, export = true)
					}
					val job = coroutineContext.job
					val bytes = PairingsFile.seal(machines, passphrase, System.currentTimeMillis(), BuildConfig.VERSION_NAME) { job.ensureActive() }
					try {
						withContext(Dispatchers.IO) {
							(resolver.openOutputStream(uri, "wt") ?: throw IOException("no stream")).use { it.write(bytes) }
						}
					} catch (e: CancellationException) {
						throw e
					} catch (e: Exception) {
						forget(uri)
						return@withContext Transfer.Failed(R.string.transfer_write_failed, export = true)
					}
					Transfer.Exported(machines.size, leftOut)
				}
			} catch (e: CancellationException) {
				// Given up on: the empty file the picker made goes too.
				withContext(NonCancellable + Dispatchers.IO) { forget(uri) }
				throw e
			} finally {
				passphrase.fill('\u0000')
			}
		}
	}

	/** The export to [uri] given up: the empty file the picker made goes. */
	fun cancelExport(uri: Uri) {
		_state.value = Transfer.Idle
		viewModelScope.launch(Dispatchers.IO) { forget(uri) }
	}

	private fun forget(uri: Uri) {
		runCatching { DocumentsContract.deleteDocument(resolver, uri) }
	}

	/** The file picked to import, or null when picking was cancelled: read, and looked at before its passphrase is asked. */
	fun importFrom(uri: Uri?) {
		if (uri == null) return
		_state.value = Transfer.Working(export = false, cancellable = false)
		viewModelScope.launch {
			val bytes = withContext(Dispatchers.IO) {
				runCatching { resolver.openInputStream(uri)?.use { readAtMost(it, PairingsFile.FILE_MAX + 1) } }.getOrNull()
			}
			_state.value = when {
				bytes == null -> Transfer.Failed(R.string.transfer_read_failed, export = false)
				bytes.size > PairingsFile.FILE_MAX -> Transfer.Failed(R.string.transfer_not_pairings, export = false)
				else -> when (val why = PairingsFile.check(bytes)) {
					null -> {
						importBytes = bytes
						Transfer.ImportNeedsPassphrase()
					}
					else -> failure(why)
				}
			}
		}
	}

	/**
	 * Opens the file read with [passphrase]. Computers it would put in place of ones here that
	 * differ from them are asked about one by one ([Transfer.ImportConflictAsked]) before anything
	 * is added; with none, it is added at once.
	 */
	fun importWith(passphrase: CharArray) {
		val bytes = importBytes ?: run {
			passphrase.fill('\u0000')
			_state.value = Transfer.Failed(R.string.transfer_read_failed, export = false)
			return
		}
		_state.value = Transfer.Working(export = false)
		work = viewModelScope.launch {
			val result = try {
				withContext(Dispatchers.Default) {
					val job = coroutineContext.job
					PairingsFile.open(bytes, passphrase) { job.ensureActive() }
				}
			} finally {
				passphrase.fill('\u0000')
			}
			when (result) {
				is PairingsFile.Opened.Pairings -> {
					importBytes = null
					opened = result.machines
					conflicts = importConflicts(store.machines.value, result.machines)
					kept.clear()
					ask(0)
				}
				PairingsFile.Opened.WrongPassphrase -> _state.value = Transfer.ImportNeedsPassphrase(wrong = true)
				else -> _state.value = failure(result)
			}
		}
	}

	/** The [index]th computer that differs asked about, or, past the last, the rest added. */
	private fun ask(index: Int) {
		if (index < conflicts.size) _state.value = Transfer.ImportConflictAsked(conflicts[index], index, conflicts.size)
		else add()
	}

	/** The computer asked about replaced ([replace]) or kept as it is here; then the next. */
	fun answer(replace: Boolean) {
		val asked = _state.value as? Transfer.ImportConflictAsked ?: return
		if (!replace) kept += asked.conflict.here.id
		ask(asked.index + 1)
	}

	/** Adds what the file holds, but for the computers kept as they are here. Not given up on midway: it is quick. */
	private fun add() {
		val machines = opened?.filter { it.id !in kept } ?: return
		opened = null
		val keptCount = kept.size
		if (machines.isEmpty()) {
			_state.value = Transfer.Imported(0, 0, keptCount)
			return
		}
		_state.value = Transfer.Working(export = false, cancellable = false)
		viewModelScope.launch {
			_state.value = try {
				// On disk before it says so.
				val merged = withContext(Dispatchers.IO) { store.addImported(machines) }
				Transfer.Imported(merged.added, merged.replaced, keptCount)
			} catch (e: KeystoreException) {
				Transfer.Failed(R.string.transfer_keystore, export = false)
			} catch (e: IOException) {
				Transfer.Failed(R.string.transfer_not_saved, export = false)
			}
		}
	}

	private fun failure(why: PairingsFile.Opened): Transfer = Transfer.Failed(
		when (why) {
			is PairingsFile.Opened.Newer -> R.string.transfer_newer
			PairingsFile.Opened.NotPairings -> R.string.transfer_not_pairings
			else -> R.string.transfer_damaged
		},
		export = false,
	)

	/** Something went wrong before the view model had a part in it, like there being no file picker. */
	fun failed(@StringRes message: Int, export: Boolean) {
		dismiss()
		_state.value = Transfer.Failed(message, export)
	}

	/** Making the key given up on: an export's file goes, an import's passphrase is asked again. */
	fun cancelWork() {
		val s = _state.value as? Transfer.Working ?: return
		if (!s.cancellable) return
		work?.cancel()
		work = null
		_state.value = if (!s.export && importBytes != null) Transfer.ImportNeedsPassphrase() else Transfer.Idle
	}

	/** Back to nothing, whatever was under way; what was held for it goes. */
	fun dismiss() {
		work?.cancel()
		work = null
		exportPassphrase?.fill('\u0000')
		exportPassphrase = null
		importBytes = null
		opened = null
		conflicts = emptyList()
		kept.clear()
		_state.value = Transfer.Idle
	}

	override fun onCleared() {
		dismiss()
	}

	companion object {
		/** What the save dialog offers to call the file. */
		fun fileName(today: LocalDate = LocalDate.now()): String = "elamon-pairings-$today.elamon"

		/** Up to [max] bytes of [input]: enough to tell a file that is too big by its length. */
		fun readAtMost(input: InputStream, max: Int): ByteArray {
			val out = ByteArrayOutputStream()
			val buf = ByteArray(BUFFER)
			while (out.size() < max) {
				val n = input.read(buf, 0, minOf(buf.size, max - out.size()))
				if (n < 0) break
				out.write(buf, 0, n)
			}
			return out.toByteArray()
		}

		private const val BUFFER = 8192
	}
}
