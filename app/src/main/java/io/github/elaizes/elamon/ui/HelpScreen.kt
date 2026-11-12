package io.github.elaizes.elamon.ui

import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
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
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.collapse
import androidx.compose.ui.semantics.expand
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import io.github.elaizes.elamon.BuildConfig
import io.github.elaizes.elamon.R
import io.github.elaizes.elamon.data.DEFAULT_HISTORY_SECONDS
import io.github.elaizes.elamon.data.DEFAULT_INTERVAL_MS
import io.github.elaizes.elamon.data.DEFAULT_PORT
import io.github.elaizes.elamon.data.Limits.MDNS_PORT
import io.github.elaizes.elamon.data.MachineStore

/** Where the project lives; the one place to change if it moves. */
object Project {
	const val URL = "https://github.com/Elaizes/elamon"
	const val GIT = "$URL.git"
	const val FLAKE = "github:Elaizes/elamon"

	/** The script that installs the newest release of elamond for the user, then runs `elamond setup`. */
	private const val INSTALL_SCRIPT = "https://raw.githubusercontent.com/Elaizes/elamon/main/install.sh"

	/** The one command that installs the service, as the guide gives it. */
	const val INSTALL = "curl -fsSL $INSTALL_SCRIPT | sh"
}

/** Help topics another screen can open the guide at. */
object HelpTopic {
	const val TROUBLESHOOTING = "troubleshooting"
}

/** The guide's parts in the order they show; a part's ordinal is its place in the list. */
private enum class HelpItem { Intro, Install, Firewall, Pair, Dashboards, Control, Faq, Commands, Settings, Privacy }

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

/** [label] is the system's own name, the same in every language, like the commands; null is "Other", from resources. */
private data class Distro(val key: String, val label: String?, val install: List<Block>, val firewall: List<Block>)

// Building elamond from source, for the distributions that run systemd: elamond setup does the rest.
private val BUILD = """
	git clone ${Project.GIT}
	cd elamon/elamond
	cargo build --release
	install -Dm755 target/release/elamond ~/.local/bin/elamond
	~/.local/bin/elamond setup
""".trimIndent()

private val FIREWALLD = "sudo firewall-cmd --permanent --add-port=$DEFAULT_PORT/tcp\nsudo firewall-cmd --permanent --add-service=mdns\nsudo firewall-cmd --reload"
private val UFW = "sudo ufw allow $DEFAULT_PORT/tcp\nsudo ufw allow $MDNS_PORT/udp"

/**
 * The service on a distribution that runs systemd: the one command, or a release by hand, both
 * ending in `elamond setup`; then from source, with [deps] the build tools of the distribution.
 */
private fun withSetup(deps: List<Block>) = listOf(
	t(R.string.help_install_one),
	c(Project.INSTALL),
	t(R.string.help_install_root),
	c("elamond setup --check"),
	t(R.string.help_install_release),
	c("~/.local/bin/elamond setup"),
	t(R.string.help_path),
	t(R.string.help_update),
	c("elamond update"),
) + deps + listOf(
	t(R.string.help_build),
	c(BUILD),
	t(R.string.help_update_source),
)

private val GENERIC_FIREWALL = listOf(
	t(R.string.help_fw_generic, ELAMOND_PORT_TEXT, MDNS_PORT_TEXT),
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
			c("imports = [ inputs.elamon.nixosModules.default ];\n\nservices.elamond = {\n  enable = true;\n  openFirewall = true;\n};\nusers.users.YOUR_USER.linger = true;"),
			t(R.string.help_nixos_rebuild),
			c("sudo nixos-rebuild switch"),
			t(R.string.help_nixos_hm),
			c("imports = [ inputs.elamon.homeModules.default ];\n\nservices.elamond.enable = true;"),
			t(R.string.help_update),
			c("nix flake update elamon\nsudo nixos-rebuild switch"),
		),
		firewall = listOf(
			t(R.string.help_fw_nixos),
			c("networking.firewall.allowedTCPPorts = [ $DEFAULT_PORT ];\nnetworking.firewall.allowedUDPPorts = [ $MDNS_PORT ];"),
		),
	),
	Distro(
		"nix", "Nix",
		install = listOf(
			t(R.string.help_nix_profile),
			c("nix profile install ${Project.FLAKE}"),
			t(R.string.help_nix_unit),
			c("systemctl --user link ~/.nix-profile/lib/systemd/user/elamond.service\nsystemctl --user enable --now elamond"),
			t(R.string.help_nix_setup),
			c("elamond setup"),
			t(R.string.help_update),
			c("nix profile upgrade --all\nsystemctl --user daemon-reload\nsystemctl --user restart elamond"),
		),
		firewall = GENERIC_FIREWALL,
	),
	Distro(
		"fedora", "Fedora",
		install = withSetup(listOf(t(R.string.help_deps), c("sudo dnf install git cargo gcc"))),
		firewall = listOf(t(R.string.help_fw_firewalld_default), c(FIREWALLD)),
	),
	Distro(
		"debian", "Debian / Ubuntu",
		install = withSetup(
			listOf(
				t(R.string.help_deps_debian),
				c("sudo apt install git build-essential curl\ncurl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh\nsource ~/.cargo/env"),
			)
		),
		firewall = listOf(t(R.string.help_fw_ufw_debian), c(UFW)),
	),
	Distro(
		"arch", "Arch",
		install = withSetup(listOf(t(R.string.help_deps), c("sudo pacman -S --needed git rust base-devel"))),
		firewall = GENERIC_FIREWALL,
	),
	Distro(
		"suse", "openSUSE",
		install = withSetup(listOf(t(R.string.help_deps), c("sudo zypper install git cargo gcc"))),
		firewall = listOf(t(R.string.help_fw_firewalld_default), c(FIREWALLD)),
	),
	Distro(
		"other", null,
		install = withSetup(listOf(t(R.string.help_deps_other))),
		firewall = GENERIC_FIREWALL,
	),
)


private const val CONTROL_NIXOS = """services.elamond = {
  enable = true;
  openFirewall = true;
  settings.control = true;
  control.users = [ "YOUR_USER" ];
};"""

private const val CONTROL_LINKS = """[links.media]
name = "Media server"
url = "http://{host}:8096"
unit = "SERVICE.service"
scope = "system"   # or "user""""

private const val CONTROL_ACTIONS = """[actions.backup]
name = "Back up now"
command = ["/path/to/backup-script", "{mode}"]
params.mode = { choices = ["quick", "full"], default = "quick" }
timeout = 600"""

/** A dashboard of one's own, a tab once a profile names it, and that profile given to a phone. */
private const val DASHBOARD = """[dashboards.power]
title = "Power"
icon = "battery"

[[dashboards.power.widgets]]
type = "status"
value = "battery.status_text"
label = "Battery"

[profiles.phone]
use = "default"
theme = "dark"
dashboards = ["overview", "power", "processes", "control"]

[profiles.phone.android]
blur = false"""
/** Where `elamond setup` saves the polkit rule, and where it goes: one name, so there are never two rules. */
private const val POLKIT_RULE = "50-elamond.rules"
private const val POLKIT_INSTALL = "sudo install -m 0644 ~/.local/share/elamon/setup/$POLKIT_RULE /etc/polkit-1/rules.d/$POLKIT_RULE"

/** The rule by hand, as the guide gives it. */
private const val CONTROL_POLKIT = """polkit.addRule(function (action, subject) {
  if (subject.user != "YOUR_USER") return polkit.Result.NOT_HANDLED;
  var power = [
    "org.freedesktop.login1.power-off",
    "org.freedesktop.login1.reboot",
    "org.freedesktop.login1.suspend",
    "org.freedesktop.UPower.PowerProfiles.switch-profile",
    "net.hadess.PowerProfiles.switch-profile"
  ];
  if (power.indexOf(action.id) >= 0) return polkit.Result.YES;
  if (action.id == "org.freedesktop.systemd1.manage-units" &&
      ["SERVICE.service"].indexOf(action.lookup("unit")) >= 0 &&
      ["start", "stop", "restart"].indexOf(action.lookup("verb")) >= 0)
    return polkit.Result.YES;
  return polkit.Result.NOT_HANDLED;
});"""
private class Faq(@StringRes val q: Int, val answer: List<Block>)

private val FAQ = listOf(
	Faq(R.string.faq_not_found_q, listOf(t(R.string.faq_not_found_a, MDNS_PORT_TEXT))),
	Faq(
		R.string.faq_no_connect_q,
		listOf(t(R.string.faq_no_connect_a), c("elamond setup --check\nsystemctl --user status elamond\njournalctl --user -u elamond -e"), t(R.string.faq_no_connect_a2, ELAMOND_PORT_TEXT)),
	),
	Faq(R.string.faq_vpn_q, listOf(t(R.string.faq_vpn_a))),
	Faq(R.string.faq_logout_q, listOf(t(R.string.faq_logout_a), c("sudo loginctl enable-linger YOUR_USER"))),
	Faq(R.string.faq_not_allowed_q, listOf(t(R.string.faq_not_allowed_a), c("elamond setup --check"))),
	Faq(R.string.faq_cert_q, listOf(t(R.string.faq_cert_a))),
	Faq(R.string.faq_sensors_q, listOf(t(R.string.faq_sensors_a), c("sudo sensors-detect\nsystemctl --user restart elamond"))),
	Faq(R.string.faq_gpu_q, listOf(t(R.string.faq_gpu_a))),
	Faq(R.string.faq_displays_q, listOf(t(R.string.faq_displays_a))),
	Faq(R.string.faq_unpair_q, listOf(t(R.string.faq_unpair_a), c("elamond clients\nelamond revoke ID"))),
	Faq(
		R.string.faq_new_phone_q,
		listOf(t(R.string.faq_new_phone_a), t(R.string.faq_new_phone_a2), c("elamond clients\nelamond revoke OLD_ID"), t(R.string.faq_new_phone_a3)),
	),
	Faq(
		R.string.faq_new_computer_q,
		listOf(
			t(R.string.faq_new_computer_a),
			c("elamond backup ~/elamond.backup"),
			t(R.string.faq_new_computer_a2),
			c("systemctl --user stop elamond\nelamond restore ~/elamond.backup\nsystemctl --user start elamond"),
			t(R.string.faq_new_computer_a3),
			c("elamond restore ~/elamond.backup --with-settings"),
		),
	),
)

private val COMMANDS = listOf(
	"elamond setup" to R.string.cmd_setup,
	"elamond setup --check" to R.string.cmd_setup_check,
	"elamond update" to R.string.cmd_update,
	"elamond pair" to R.string.cmd_pair,
	"elamond clients" to R.string.cmd_clients,
	"elamond clients role ID ROLE" to R.string.cmd_clients_role,
	"elamond clients profile ID PROFILE" to R.string.cmd_clients_profile,
	"elamond revoke ID" to R.string.cmd_revoke,
	"elamond revoke all" to R.string.cmd_revoke_all,
	"elamond control off" to R.string.cmd_control_off,
	"elamond defaults" to R.string.cmd_defaults,
	"elamond defaults --diff" to R.string.cmd_defaults_diff,
	"elamond check" to R.string.cmd_check,
	"elamond edit" to R.string.cmd_edit,
	"elamond values" to R.string.cmd_values,
	"elamond rotate-cert" to R.string.cmd_rotate_cert,
	"elamond backup FILE" to R.string.cmd_backup,
	"elamond restore FILE" to R.string.cmd_restore,
	"elamond snapshot" to R.string.cmd_snapshot,
	"elamond --version" to R.string.cmd_version,
)

/** The service's settings with its defaults. */
private val SETTINGS = "port = $DEFAULT_PORT\nlisten = [\"::\"]\nnames = []\nname = \"\"\nmdns = true\ninterval_ms = $DEFAULT_INTERVAL_MS\nhistory_minutes = ${DEFAULT_HISTORY_SECONDS / 60}"

private val SETTING_NOTES = listOf(
	"port" to R.string.set_port,
	"listen" to R.string.set_listen,
	"names" to R.string.set_names,
	"name" to R.string.set_name,
	"mdns" to R.string.set_mdns,
	"interval_ms" to R.string.set_interval,
	"history_minutes" to R.string.set_history,
)

/** The first paired computer's distribution, so the guide opens on the right commands. */
private fun guessDistro(context: Context): String {
	val os = MachineStore.get(context).machines.value.firstNotNullOfOrNull { it.os }?.lowercase() ?: return "debian"
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
					HelpItem.Install -> {
						Section(stringResource(R.string.help_install), ElIcons.Laptop) {
							Text(stringResource(R.string.help_pick_system), style = MaterialTheme.typography.bodyMedium)
							FlowRow(horizontalArrangement = Arrangement.spacedBy(Space.s)) {
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
							Text(stringResource(R.string.help_fw_why, ELAMOND_PORT_TEXT, MDNS_PORT_TEXT), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
						}
					}
					HelpItem.Pair -> {
						Section(stringResource(R.string.help_pair), ElIcons.Qr) {
							Blocks(
								listOf(
									t(R.string.help_pair_run),
									c("elamond pair"),
									t(R.string.help_pair_scan),
									t(R.string.help_pair_code),
									t(R.string.help_pair_allow),
									t(R.string.help_pair_role),
									c("elamond pair --role glance"),
								)
							)
						}
					}
					HelpItem.Dashboards -> {
						Section(stringResource(R.string.help_dashboards), ElIcons.Overview) {
							Blocks(
								listOf(
									t(R.string.help_dashboards_intro),
									c("elamond defaults\nelamond defaults --diff"),
									t(R.string.help_dashboards_files),
									c(DASHBOARD),
									t(R.string.help_dashboards_check),
									c("elamond check\nelamond values"),
									t(R.string.help_dashboards_profile),
									c("elamond clients\nelamond clients profile ID phone"),
									t(R.string.help_dashboards_app),
								)
							)
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
									t(R.string.help_control_links),
									c(CONTROL_LINKS),
									t(R.string.help_control_actions),
									c(CONTROL_ACTIONS),
									t(R.string.help_control_polkit),
									c(POLKIT_INSTALL),
									t(R.string.help_control_polkit_hand),
									c(CONTROL_POLKIT),
									t(R.string.help_control_unlock),
								)
							)
						}
					}
					HelpItem.Faq -> {
						Section(stringResource(R.string.help_troubleshooting), Icons.Default.Build) {
							FAQ.forEachIndexed { i, f ->
								if (i > 0) HorizontalDivider(color = MaterialTheme.colorScheme.outlineVariant.copy(alpha = Alpha.DIVIDER_STRONG))
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

/** A command or setting in monospace with what it does underneath. */
@Composable
private fun Reference(name: String, description: String) {
	Column(verticalArrangement = Arrangement.spacedBy(Space.xxs)) {
		Text(name, fontFamily = FontFamily.Monospace, style = MaterialTheme.typography.bodyMedium, fontWeight = FontWeight.Medium)
		Text(description, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
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

@Composable
private fun FaqItem(f: Faq) {
	var open by rememberSaveable(f.q) { mutableStateOf(false) }
	val state = stringResource(if (open) R.string.a11y_expanded else R.string.a11y_collapsed)
	Column {
		Row(
			Modifier
				.fillMaxWidth()
				.clickable(role = Role.Button) { open = !open }
				// Says whether the answer shows, and offers to show or hide it.
				.semantics {
					stateDescription = state
					if (open) collapse { open = false; true } else expand { open = true; true }
				}
				.padding(vertical = Space.sPlus),
			verticalAlignment = Alignment.CenterVertically,
		) {
			Text(stringResource(f.q), style = MaterialTheme.typography.bodyLarge, fontWeight = FontWeight.Medium, modifier = Modifier.weight(1f))
			Icon(if (open) Icons.Default.KeyboardArrowUp else Icons.Default.KeyboardArrowDown, null)
		}
		AnimatedVisibility(open) {
			Column(Modifier.padding(bottom = Space.m)) { Blocks(f.answer) }
		}
	}
}

internal fun copy(ctx: Context, text: String) {
	val cm = ctx.getSystemService(Context.CLIPBOARD_SERVICE) as ClipboardManager
	cm.setPrimaryClip(ClipData.newPlainText(ctx.getString(R.string.app_name), text))
	// Android 13 and newer show their own confirmation.
	if (Build.VERSION.SDK_INT < 33) Toast.makeText(ctx, R.string.copied, Toast.LENGTH_SHORT).show()
}

private fun openUrl(ctx: Context, url: String) {
	openWebLink(ctx, url)
}
