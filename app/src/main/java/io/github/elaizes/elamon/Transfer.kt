package io.github.elaizes.elamon

import android.app.Application
import android.net.Uri
import android.provider.DocumentsContract
import androidx.annotation.StringRes
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import io.github.elaizes.elamon.data.KeystoreException
import io.github.elaizes.elamon.data.MachineStore
import io.github.elaizes.elamon.data.PairingsFile
import java.io.ByteArrayOutputStream
import java.io.IOException
import java.io.InputStream
import java.time.LocalDate
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

/** Where exporting or importing pairings is, for Settings to show. */
sealed interface Transfer {
	data object Idle : Transfer

	/** The file to write was chosen, but the passphrase was lost meanwhile: the app was closed while the file was picked. */
	data class ExportNeedsPassphrase(val uri: Uri) : Transfer

	/** A file of pairings was read; its passphrase is next. [wrong] after one that didn't open it. */
	data class ImportNeedsPassphrase(val wrong: Boolean = false) : Transfer

	/** Making the key and sealing or opening: a second or two. */
	data class Working(val export: Boolean) : Transfer

	data class Exported(val count: Int, val leftOut: Int) : Transfer

	data class Imported(val added: Int, val replaced: Int) : Transfer

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
		viewModelScope.launch {
			_state.value = try {
				withContext(Dispatchers.Default) {
					val (machines, leftOut) = store.withTokens()
					if (machines.isEmpty()) {
						forget(uri)
						return@withContext Transfer.Failed(R.string.transfer_nothing_to_export, export = true)
					}
					val bytes = PairingsFile.seal(machines, passphrase, System.currentTimeMillis(), BuildConfig.VERSION_NAME)
					try {
						withContext(Dispatchers.IO) {
							(resolver.openOutputStream(uri, "wt") ?: throw IOException("no stream")).use { it.write(bytes) }
						}
					} catch (e: Exception) {
						forget(uri)
						return@withContext Transfer.Failed(R.string.transfer_write_failed, export = true)
					}
					Transfer.Exported(machines.size, leftOut)
				}
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
		_state.value = Transfer.Working(export = false)
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

	/** Opens the file read with [passphrase] and adds what it holds. */
	fun importWith(passphrase: CharArray) {
		val bytes = importBytes ?: run {
			passphrase.fill('\u0000')
			_state.value = Transfer.Failed(R.string.transfer_read_failed, export = false)
			return
		}
		_state.value = Transfer.Working(export = false)
		viewModelScope.launch {
			_state.value = try {
				withContext(Dispatchers.Default) {
					when (val opened = PairingsFile.open(bytes, passphrase)) {
						is PairingsFile.Opened.Pairings -> try {
							// On disk before it says so.
							val merged = withContext(Dispatchers.IO) { store.addImported(opened.machines) }
							importBytes = null
							Transfer.Imported(merged.added, merged.replaced)
						} catch (e: KeystoreException) {
							Transfer.Failed(R.string.transfer_keystore, export = false)
						} catch (e: IOException) {
							Transfer.Failed(R.string.transfer_not_saved, export = false)
						}
						PairingsFile.Opened.WrongPassphrase -> Transfer.ImportNeedsPassphrase(wrong = true)
						else -> failure(opened)
					}
				}
			} finally {
				passphrase.fill('\u0000')
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

	/** Back to nothing, whatever was under way; what was held for it goes. */
	fun dismiss() {
		exportPassphrase?.fill('\u0000')
		exportPassphrase = null
		importBytes = null
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
