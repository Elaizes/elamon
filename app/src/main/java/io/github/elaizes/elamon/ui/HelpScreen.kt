package io.github.elaizes.elamon.ui

import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
import android.os.Build
import android.widget.Toast
import androidx.annotation.StringRes
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.filled.Edit
import androidx.compose.material.icons.filled.Info
import androidx.compose.material.icons.filled.Lock
import androidx.compose.material.icons.filled.Notifications
import androidx.compose.material.icons.filled.Settings
import androidx.compose.material3.Button
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontFamily
import io.github.elaizes.elamon.BuildConfig
import io.github.elaizes.elamon.R
import io.github.elaizes.elamon.data.DEFAULT_PORT
import io.github.elaizes.elamon.data.Limits.MDNS_PORT

/** Where the project lives; the one place to change if it moves. */
object Project {
	const val URL = "https://github.com/Elaizes/elamon"
	const val FLAKE = "github:Elaizes/elamon"

	/** The full guide, which is online only. */
	const val GUIDE = "$URL/blob/main/docs/guide.md"

	/** The script that installs the newest release of elamond for the user, then runs `elamond setup`. */
	private const val INSTALL_SCRIPT = "https://raw.githubusercontent.com/Elaizes/elamon/main/install.sh"

	/** The one command that installs the service, as the guide gives it. */
	const val INSTALL = "curl -fsSL $INSTALL_SCRIPT | sh"
}

/** The help's parts in the order they show. */
private enum class HelpItem { Intro, Install, Firewall, Pair, Phone, Outside, Events, Editing, Guide }

private sealed interface Block {
	/** A paragraph; [args] fill its placeholders. */
	data class Text(@StringRes val res: Int, val args: List<String> = emptyList()) : Block
	data class Code(val code: String) : Block
}

private fun t(@StringRes res: Int, vararg args: String) = Block.Text(res, args.toList())
private fun c(code: String) = Block.Code(code)

/**
 * The ports the guide's words name, from the same constants as its commands. As text: formatted as
 * numbers they could come out in the phone's own digits, and they are to be typed as they are.
 */
private val ELAMOND_PORT_TEXT = DEFAULT_PORT.toString()
private val MDNS_PORT_TEXT = MDNS_PORT.toString()

private val FIREWALLD = "sudo firewall-cmd --permanent --add-port=$DEFAULT_PORT/tcp\nsudo firewall-cmd --permanent --add-service=mdns\nsudo firewall-cmd --reload"
private val UFW = "sudo ufw allow $DEFAULT_PORT/tcp\nsudo ufw allow $MDNS_PORT/udp"

/** The one command that installs the service, and the NixOS module for those who have it instead. */
private val INSTALL = listOf(
	t(R.string.help_install_one),
	c(Project.INSTALL),
	t(R.string.help_nixos_input),
	c("inputs.elamon = {\n  url = \"${Project.FLAKE}\";\n  inputs.nixpkgs.follows = \"nixpkgs\";\n};"),
	t(R.string.help_nixos_module),
	c("imports = [ inputs.elamon.nixosModules.default ];\n\nservices.elamond = {\n  enable = true;\n  openFirewall = true;\n};\nusers.users.YOUR_USER.linger = true;"),
)

private val FIREWALL = listOf(
	t(R.string.help_fw_generic, ELAMOND_PORT_TEXT, MDNS_PORT_TEXT),
	t(R.string.help_fw_ufw),
	c(UFW),
	t(R.string.help_fw_firewalld),
	c(FIREWALLD),
	t(R.string.help_fw_nixos),
	c("networking.firewall.allowedTCPPorts = [ $DEFAULT_PORT ];\nnetworking.firewall.allowedUDPPorts = [ $MDNS_PORT ];"),
)

/** What the computer's profile sets, what is this phone's own, and the monitors in Control. */
private val PHONE = listOf(
	t(R.string.help_phone_profile),
	t(R.string.help_phone_own),
	t(R.string.help_phone_monitors),
)

/**
 * The home-screen widget, the Quick Settings tiles and the computer's card: what each shows, and that
 * the computer's profile says which dashboards.
 */
private val OUTSIDE = listOf(
	t(R.string.help_outside_widget),
	t(R.string.help_outside_tiles),
	t(R.string.help_outside_card),
	t(R.string.help_outside_settings),
	c("[profiles.<id>.android]\nwidget = \"phone\"\nwidget_minutes = 30\ntiles = \"tiles\"\ncard = \"card\""),
)

/** What the computer tells of itself, where the app shows it, and that notifications are ntfy's. */
private val EVENTS = listOf(
	t(R.string.help_events_what),
	t(R.string.help_events_ntfy),
	t(R.string.help_events_settings),
	c("elamond notify \"Backup finished\""),
)

/**
 * The dashboard editor (elamond 0.11): who has it, how a dashboard is changed, unlocking before saving,
 * the file the computer writes, Back to the files, and the web page for the rest of the admin panel.
 */
private val EDITING = listOf(
	t(R.string.help_editor_who),
	t(R.string.help_editor_how),
	t(R.string.help_editor_save),
	c("~/.config/elamon/elamond.d/zz-editor.toml"),
	t(R.string.help_editor_back),
	t(R.string.help_editor_web),
	c("https://COMPUTER:$DEFAULT_PORT/"),
)

private val PAIR = listOf(
	t(R.string.help_pair_run),
	c("elamond pair"),
	t(R.string.help_pair_scan),
	t(R.string.help_pair_code),
	t(R.string.help_pair_allow),
)

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun HelpScreen(onBack: () -> Unit) {
	val ctx = LocalContext.current

	Scaffold(
		topBar = {
			TopAppBar(
				title = { Text(stringResource(R.string.help_screen_title)) },
				navigationIcon = { IconButton(onClick = onBack) { Icon(Icons.AutoMirrored.Filled.ArrowBack, stringResource(R.string.back)) } },
			)
		},
	) { padding ->
		LazyColumn(
			Modifier.fillMaxSize(),
			contentPadding = PaddingValues(Space.l, padding.calculateTopPadding() + Space.s, Space.l, padding.calculateBottomPadding() + Space.xxl),
			verticalArrangement = Arrangement.spacedBy(Space.m),
		) {
			for (part in HelpItem.entries) item(key = part.name) {
				when (part) {
					HelpItem.Intro -> {
						Text(
							stringResource(R.string.help_intro),
							style = MaterialTheme.typography.bodyLarge,
							color = MaterialTheme.colorScheme.onSurfaceVariant,
							modifier = Modifier.padding(horizontal = Space.xs),
						)
					}
					HelpItem.Install -> Section(stringResource(R.string.help_install), ElIcons.Laptop) { Blocks(INSTALL) }
					HelpItem.Firewall -> Section(stringResource(R.string.help_firewall), Icons.Default.Lock) { Blocks(FIREWALL) }
					HelpItem.Pair -> Section(stringResource(R.string.help_pair), ElIcons.Qr) { Blocks(PAIR) }
					HelpItem.Phone -> Section(stringResource(R.string.help_phone), Icons.Default.Settings) { Blocks(PHONE) }
					HelpItem.Outside -> Section(stringResource(R.string.help_outside), ElIcons.Control) { Blocks(OUTSIDE) }
					HelpItem.Events -> Section(stringResource(R.string.help_events), Icons.Default.Notifications) { Blocks(EVENTS) }
					HelpItem.Editing -> Section(stringResource(R.string.help_editor), Icons.Default.Edit) { Blocks(EDITING) }
					HelpItem.Guide -> {
						Section(stringResource(R.string.help_guide), Icons.Default.Info) {
							Text(stringResource(R.string.help_guide_body), style = MaterialTheme.typography.bodyMedium)
							Button(onClick = { openWebLink(ctx, Project.GUIDE) }) { Text(stringResource(R.string.help_guide_button)) }
							Text(
								stringResource(R.string.help_version, BuildConfig.VERSION_NAME),
								style = MaterialTheme.typography.bodySmall,
								color = MaterialTheme.colorScheme.onSurfaceVariant,
							)
							OutlinedButton(onClick = { openWebLink(ctx, Project.URL) }) { Text(stringResource(R.string.help_source)) }
						}
					}
				}
			}
		}
	}
}

@Composable
private fun Blocks(blocks: List<Block>) {
	Column(verticalArrangement = Arrangement.spacedBy(Space.sPlus)) {
		for (b in blocks) {
			when (b) {
				// Without arguments the text isn't formatted at all, so a "%" in it stays as written.
				is Block.Text -> Text(if (b.args.isEmpty()) stringResource(b.res) else stringResource(b.res, *b.args.toTypedArray()), style = MaterialTheme.typography.bodyMedium)
				is Block.Code -> CodeBlock(b.code)
			}
		}
	}
}

/** A command or config snippet: scrolls sideways instead of wrapping, copies with one tap. */
@Composable
fun CodeBlock(code: String) {
	val ctx = LocalContext.current
	Surface(shape = RoundedCornerShape(Radius.m), color = MaterialTheme.colorScheme.surfaceContainerHighest, modifier = Modifier.fillMaxWidth()) {
		Row(verticalAlignment = Alignment.Top) {
			Text(
				code,
				fontFamily = FontFamily.Monospace,
				style = MaterialTheme.typography.bodySmall,
				softWrap = false,
				modifier = Modifier.weight(1f).horizontalScroll(rememberScrollState()).padding(start = Space.m, top = Space.m, bottom = Space.m, end = Space.xs),
			)
			IconButton(onClick = { copy(ctx, code) }) {
				Icon(ElIcons.Copy, stringResource(R.string.copy), modifier = Modifier.size(Sizes.iconM), tint = MaterialTheme.colorScheme.onSurfaceVariant)
			}
		}
	}
}

internal fun copy(ctx: Context, text: String) {
	val cm = ctx.getSystemService(Context.CLIPBOARD_SERVICE) as ClipboardManager
	cm.setPrimaryClip(ClipData.newPlainText(ctx.getString(R.string.app_name), text))
	// Android 13 and newer show their own confirmation.
	if (Build.VERSION.SDK_INT < 33) Toast.makeText(ctx, R.string.copied, Toast.LENGTH_SHORT).show()
}
