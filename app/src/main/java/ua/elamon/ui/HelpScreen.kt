package ua.elamon.ui

import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
import android.content.Intent
import android.net.Uri
import android.os.Build
import android.widget.Toast
import androidx.annotation.StringRes
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.foundation.clickable
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.filled.Build
import androidx.compose.material.icons.filled.Info
import androidx.compose.material.icons.filled.KeyboardArrowDown
import androidx.compose.material.icons.filled.KeyboardArrowUp
import androidx.compose.material.icons.filled.Lock
import androidx.compose.material.icons.filled.Settings
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.FilterChip
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import ua.elamon.BuildConfig
import ua.elamon.R
import ua.elamon.data.ServerStore

/** Where the project lives; the one place to change if it moves. */
object Project {
	const val URL = "https://github.com/Elaizes/elamon"
	const val GIT = "$URL.git"
	const val FLAKE = "github:Elaizes/elamon"
}

/** Help topics another screen can open the guide at. */
object HelpTopic {
	const val TROUBLESHOOTING = "troubleshooting"
}

/** The guide's parts in the order they show; a part's ordinal is its place in the list. */
private enum class HelpItem { Intro, Install, Firewall, Pair, Control, Faq, Commands, Settings, Privacy }

private sealed interface Block {
	data class Text(@StringRes val res: Int) : Block
	data class Code(val code: String) : Block
}

private fun t(@StringRes res: Int) = Block.Text(res)
private fun c(code: String) = Block.Code(code)

/** [label] is the system's own name, the same in every language, like the commands; null is "Other", from resources. */
private data class Distro(val key: String, val label: String?, val install: List<Block>, val firewall: List<Block>)

// Commands shared by the distributions that build the service from source.
private val BUILD = """
	git clone ${Project.GIT}
	cd elamon/elamond
	cargo build --release
	install -Dm755 target/release/elamond ~/.local/bin/elamond
	install -Dm644 dist/elamond.service ~/.config/systemd/user/elamond.service
	sed -i "s|/usr/bin/elamond|${'$'}HOME/.local/bin/elamond|" ~/.config/systemd/user/elamond.service
	systemctl --user daemon-reload
	systemctl --user enable --now elamond
""".trimIndent()

private val UPDATE = """
	cd elamon
	git pull
	cd elamond && cargo build --release
	install -Dm755 target/release/elamond ~/.local/bin/elamond
	systemctl --user restart elamond
""".trimIndent()

private const val FIREWALLD = "sudo firewall-cmd --permanent --add-port=47823/tcp\nsudo firewall-cmd --permanent --add-service=mdns\nsudo firewall-cmd --reload"
private const val UFW = "sudo ufw allow 47823/tcp\nsudo ufw allow 5353/udp"

private fun fromSource(deps: List<Block>) = deps + listOf(
	t(R.string.help_build),
	c(BUILD),
	t(R.string.help_linger),
	c("loginctl enable-linger"),
	t(R.string.help_path),
	t(R.string.help_update),
	c(UPDATE),
)

private val GENERIC_FIREWALL = listOf(
	t(R.string.help_fw_generic),
	t(R.string.help_fw_ufw),
	c(UFW),
	t(R.string.help_fw_firewalld),
	c(FIREWALLD),
)

private val DISTROS = listOf(
	Distro(
		"nixos", "NixOS",
		install = listOf(
			t(R.string.help_nixos_input),
			c("inputs.elamon = {\n  url = \"${Project.FLAKE}\";\n  inputs.nixpkgs.follows = \"nixpkgs\";\n};"),
			t(R.string.help_nixos_module),
			c("imports = [ inputs.elamon.nixosModules.default ];\n\nservices.elamond = {\n  enable = true;\n  openFirewall = true;\n};"),
			t(R.string.help_nixos_rebuild),
			c("sudo nixos-rebuild switch"),
			t(R.string.help_linger),
			c("loginctl enable-linger"),
			t(R.string.help_nixos_hm),
			c("imports = [ inputs.elamon.homeManagerModules.default ];\n\nservices.elamond.enable = true;"),
		),
		firewall = listOf(
			t(R.string.help_fw_nixos),
			c("networking.firewall.allowedTCPPorts = [ 47823 ];\nnetworking.firewall.allowedUDPPorts = [ 5353 ];"),
		),
	),
	Distro(
		"nix", "Nix",
		install = listOf(
			t(R.string.help_nix_profile),
			c("nix profile install ${Project.FLAKE}"),
			t(R.string.help_nix_unit),
			c("systemctl --user link ~/.nix-profile/lib/systemd/user/elamond.service\nsystemctl --user enable --now elamond"),
			t(R.string.help_linger),
			c("loginctl enable-linger"),
			t(R.string.help_update),
			c("nix profile upgrade --all\nsystemctl --user daemon-reload\nsystemctl --user restart elamond"),
		),
		firewall = GENERIC_FIREWALL,
	),
	Distro(
		"fedora", "Fedora",
		install = fromSource(listOf(t(R.string.help_deps), c("sudo dnf install git cargo gcc"))),
		firewall = listOf(t(R.string.help_fw_firewalld_default), c(FIREWALLD)),
	),
	Distro(
		"debian", "Debian / Ubuntu",
		install = fromSource(
			listOf(
				t(R.string.help_deps_debian),
				c("sudo apt install git build-essential curl\ncurl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh\nsource ~/.cargo/env"),
			)
		),
		firewall = listOf(t(R.string.help_fw_ufw_debian), c(UFW)),
	),
	Distro(
		"arch", "Arch",
		install = fromSource(listOf(t(R.string.help_deps), c("sudo pacman -S --needed git rust base-devel"))),
		firewall = GENERIC_FIREWALL,
	),
	Distro(
		"suse", "openSUSE",
		install = fromSource(listOf(t(R.string.help_deps), c("sudo zypper install git cargo gcc"))),
		firewall = listOf(t(R.string.help_fw_firewalld_default), c(FIREWALLD)),
	),
	Distro(
		"other", null,
		install = fromSource(listOf(t(R.string.help_deps_other))),
		firewall = GENERIC_FIREWALL,
	),
)


private const val CONTROL_NIXOS = """services.elamond = {
  enable = true;
  openFirewall = true;
  settings.control = true;
  control.users = [ "YOUR_USER" ];
  control.units = [ "SERVICE.service" ];
};"""

private const val CONTROL_SERVICES = """[[services]]
name = "Media server"
url = "http://{host}:8096"
unit = "SERVICE.service"
scope = "system"   # or "user""""

private const val CONTROL_ACTIONS = """[[actions]]
name = "Back up now"
command = ["/path/to/backup-script", "--now"]
confirm = true
timeout = 600"""

private const val CONTROL_POLKIT = """polkit.addRule(function (action, subject) {
  if (subject.user != "YOUR_USER") return polkit.Result.NOT_HANDLED;
  var id = action.id;
  if (id.indexOf("org.freedesktop.login1.power-off") == 0 ||
      id.indexOf("org.freedesktop.login1.reboot") == 0 ||
      id.indexOf("org.freedesktop.login1.suspend") == 0 ||
      id == "org.freedesktop.UPower.PowerProfiles.switch-profile" ||
      id == "net.hadess.PowerProfiles.switch-profile")
    return polkit.Result.YES;
  if (id == "org.freedesktop.systemd1.manage-units" &&
      ["SERVICE.service"].indexOf(action.lookup("unit")) >= 0)
    return polkit.Result.YES;
  return polkit.Result.NOT_HANDLED;
});"""
private class Faq(@StringRes val q: Int, val answer: List<Block>)

private val FAQ = listOf(
	Faq(R.string.faq_not_found_q, listOf(t(R.string.faq_not_found_a))),
	Faq(
		R.string.faq_no_connect_q,
		listOf(t(R.string.faq_no_connect_a), c("systemctl --user status elamond\njournalctl --user -u elamond -e"), t(R.string.faq_no_connect_a2)),
	),
	Faq(R.string.faq_logout_q, listOf(t(R.string.faq_logout_a), c("loginctl enable-linger"))),
	Faq(R.string.faq_cert_q, listOf(t(R.string.faq_cert_a))),
	Faq(R.string.faq_sensors_q, listOf(t(R.string.faq_sensors_a), c("sudo sensors-detect\nsystemctl --user restart elamond"))),
	Faq(R.string.faq_gpu_q, listOf(t(R.string.faq_gpu_a))),
	Faq(R.string.faq_displays_q, listOf(t(R.string.faq_displays_a))),
	Faq(R.string.faq_unpair_q, listOf(t(R.string.faq_unpair_a), c("elamond clients\nelamond revoke ID"))),
)

private val COMMANDS = listOf(
	"elamond pair" to R.string.cmd_pair,
	"elamond clients" to R.string.cmd_clients,
	"elamond revoke ID" to R.string.cmd_revoke,
	"elamond revoke all" to R.string.cmd_revoke_all,
	"elamond snapshot" to R.string.cmd_snapshot,
	"elamond --version" to R.string.cmd_version,
)

private const val SETTINGS = "port = 47823\nlisten = \"::\"\nname = \"\"\nmdns = true\ninterval_ms = 1000\nhistory_minutes = 10"

private val SETTING_NOTES = listOf(
	"port" to R.string.set_port,
	"listen" to R.string.set_listen,
	"name" to R.string.set_name,
	"mdns" to R.string.set_mdns,
	"interval_ms" to R.string.set_interval,
	"history_minutes" to R.string.set_history,
)

/** The first paired computer's distribution, so the guide opens on the right commands. */
private fun guessDistro(context: Context): String {
	val os = ServerStore.get(context).servers.value.firstNotNullOfOrNull { it.os }?.lowercase() ?: return "debian"
	return when {
		"nixos" in os -> "nixos"
		"fedora" in os || "nobara" in os || "bazzite" in os -> "fedora"
		listOf("arch", "manjaro", "endeavour", "cachy", "garuda").any { it in os } -> "arch"
		"suse" in os -> "suse"
		listOf("debian", "ubuntu", "mint", "pop!_os", "elementary", "zorin", "kali").any { it in os } -> "debian"
		else -> "other"
	}
}

@OptIn(ExperimentalMaterial3Api::class, ExperimentalLayoutApi::class)
@Composable
fun HelpScreen(topic: String?, onBack: () -> Unit) {
	val ctx = LocalContext.current
	var distroKey by rememberSaveable { mutableStateOf(guessDistro(ctx)) }
	val distro = DISTROS.first { it.key == distroKey }
	val list = rememberLazyListState()

	// Opened at a topic: scrolled there once, not again after a rotation, which keeps its own place.
	var scrolled by rememberSaveable { mutableStateOf(false) }
	LaunchedEffect(topic) {
		if (topic == HelpTopic.TROUBLESHOOTING && !scrolled) list.scrollToItem(HelpItem.Faq.ordinal)
		scrolled = true
	}

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
			state = list,
			contentPadding = PaddingValues(16.dp, padding.calculateTopPadding() + 8.dp, 16.dp, padding.calculateBottomPadding() + 24.dp),
			verticalArrangement = Arrangement.spacedBy(12.dp),
		) {
			for (part in HelpItem.entries) item(key = part.name) {
				when (part) {
					HelpItem.Intro -> {
						Text(
							stringResource(R.string.help_intro),
							style = MaterialTheme.typography.bodyLarge,
							color = MaterialTheme.colorScheme.onSurfaceVariant,
							modifier = Modifier.padding(horizontal = 4.dp),
						)
					}
					HelpItem.Install -> {
						Section(stringResource(R.string.help_install), ElIcons.Laptop) {
							Text(stringResource(R.string.help_pick_system), style = MaterialTheme.typography.bodyMedium)
							FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
								for (d in DISTROS) {
									FilterChip(
										selected = d.key == distroKey,
										onClick = { distroKey = d.key },
										label = { Text(d.label ?: stringResource(R.string.help_other_system)) },
									)
								}
							}
							Blocks(distro.install)
						}
					}
					HelpItem.Firewall -> {
						Section(stringResource(R.string.help_firewall), Icons.Default.Lock) {
							Blocks(distro.firewall)
							Text(stringResource(R.string.help_fw_why), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
						}
					}
					HelpItem.Pair -> {
						Section(stringResource(R.string.help_pair), ElIcons.Qr) {
							Blocks(listOf(t(R.string.help_pair_run), c("elamond pair"), t(R.string.help_pair_scan), t(R.string.help_pair_code)))
						}
					}
					HelpItem.Control -> {
						Section(stringResource(R.string.help_control), ElIcons.Control) {
							Blocks(
								listOf(
									t(R.string.help_control_intro),
									c("# ~/.config/elamon/elamond.toml\ncontrol = true"),
									t(R.string.help_control_nixos),
									c(CONTROL_NIXOS),
									t(R.string.help_control_services),
									c(CONTROL_SERVICES),
									t(R.string.help_control_actions),
									c(CONTROL_ACTIONS),
									t(R.string.help_control_polkit),
									c(CONTROL_POLKIT),
								)
							)
						}
					}
					HelpItem.Faq -> {
						Section(stringResource(R.string.help_troubleshooting), Icons.Default.Build) {
							FAQ.forEachIndexed { i, f ->
								if (i > 0) HorizontalDivider(color = MaterialTheme.colorScheme.outlineVariant.copy(alpha = 0.5f))
								FaqItem(f)
							}
						}
					}
					HelpItem.Commands -> {
						Section(stringResource(R.string.help_commands), ElIcons.Processes) {
							for ((cmd, desc) in COMMANDS) Reference(cmd, stringResource(desc))
						}
					}
					HelpItem.Settings -> {
						Section(stringResource(R.string.help_settings), Icons.Default.Settings) {
							Blocks(listOf(t(R.string.help_settings_where), c(SETTINGS)))
							for ((key, desc) in SETTING_NOTES) Reference(key, stringResource(desc))
							Text(stringResource(R.string.help_settings_state), style = MaterialTheme.typography.bodyMedium)
						}
					}
					HelpItem.Privacy -> {
						Section(stringResource(R.string.help_privacy), Icons.Default.Info) {
							Text(stringResource(R.string.help_privacy_body), style = MaterialTheme.typography.bodyMedium)
							Text(
								stringResource(R.string.help_version, BuildConfig.VERSION_NAME),
								style = MaterialTheme.typography.bodySmall,
								color = MaterialTheme.colorScheme.onSurfaceVariant,
							)
							OutlinedButton(onClick = { openUrl(ctx, Project.URL) }) { Text(stringResource(R.string.help_source)) }
						}
					}
				}
			}
		}
	}
}

@Composable
private fun Blocks(blocks: List<Block>) {
	Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
		for (b in blocks) {
			when (b) {
				is Block.Text -> Text(stringResource(b.res), style = MaterialTheme.typography.bodyMedium)
				is Block.Code -> CodeBlock(b.code)
			}
		}
	}
}

/** A command or setting in monospace with what it does underneath. */
@Composable
private fun Reference(name: String, description: String) {
	Column(verticalArrangement = Arrangement.spacedBy(2.dp)) {
		Text(name, fontFamily = FontFamily.Monospace, style = MaterialTheme.typography.bodyMedium, fontWeight = FontWeight.Medium)
		Text(description, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
	}
}

/** A command or config snippet: scrolls sideways instead of wrapping, copies with one tap. */
@Composable
fun CodeBlock(code: String) {
	val ctx = LocalContext.current
	Surface(shape = RoundedCornerShape(12.dp), color = MaterialTheme.colorScheme.surfaceContainerHighest, modifier = Modifier.fillMaxWidth()) {
		Row(verticalAlignment = Alignment.Top) {
			Text(
				code,
				fontFamily = FontFamily.Monospace,
				style = MaterialTheme.typography.bodySmall,
				softWrap = false,
				modifier = Modifier.weight(1f).horizontalScroll(rememberScrollState()).padding(start = 12.dp, top = 12.dp, bottom = 12.dp, end = 4.dp),
			)
			IconButton(onClick = { copy(ctx, code) }) {
				Icon(ElIcons.Copy, stringResource(R.string.copy), modifier = Modifier.size(18.dp), tint = MaterialTheme.colorScheme.onSurfaceVariant)
			}
		}
	}
}

@Composable
private fun FaqItem(f: Faq) {
	var open by rememberSaveable(f.q) { mutableStateOf(false) }
	Column {
		Row(
			Modifier.fillMaxWidth().clickable { open = !open }.padding(vertical = 10.dp),
			verticalAlignment = Alignment.CenterVertically,
		) {
			Text(stringResource(f.q), style = MaterialTheme.typography.bodyLarge, fontWeight = FontWeight.Medium, modifier = Modifier.weight(1f))
			Icon(if (open) Icons.Default.KeyboardArrowUp else Icons.Default.KeyboardArrowDown, null)
		}
		AnimatedVisibility(open) {
			Column(Modifier.padding(bottom = 12.dp)) { Blocks(f.answer) }
		}
	}
}

private fun copy(ctx: Context, text: String) {
	val cm = ctx.getSystemService(Context.CLIPBOARD_SERVICE) as ClipboardManager
	cm.setPrimaryClip(ClipData.newPlainText(ctx.getString(R.string.app_name), text))
	// Android 13 and newer show their own confirmation.
	if (Build.VERSION.SDK_INT < 33) Toast.makeText(ctx, R.string.copied, Toast.LENGTH_SHORT).show()
}

private fun openUrl(ctx: Context, url: String) {
	runCatching { ctx.startActivity(Intent(Intent.ACTION_VIEW, Uri.parse(url))) }
}
