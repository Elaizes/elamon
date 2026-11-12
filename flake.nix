{
	description = "Elamon — watch and control a computer from an Android phone";

	inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

	outputs = { self, nixpkgs, ... }:
		let
			# elamond runs on PCs and on ARM boards alike.
			systems = [ "x86_64-linux" "aarch64-linux" ];
			forAll = f: nixpkgs.lib.genAttrs systems (s: f nixpkgs.legacyPackages.${s});
			cargoToml = builtins.fromTOML (builtins.readFile ./elamond/Cargo.toml);

			elamondFor = pkgs: pkgs.rustPlatform.buildRustPackage {
				pname = "elamond";
				version = cargoToml.package.version;
				# Its own store path, so commits that touch only the app leave elamond as built.
				src = builtins.path {
					path = ./elamond;
					name = "elamond-source";
				};
				cargoLock.lockFile = ./elamond/Cargo.lock;
				nativeBuildInputs = [ pkgs.makeWrapper ];
				# `elamond update`'s tests download, with curl, from a server of their own on 127.0.0.1.
				nativeCheckInputs = [ pkgs.curl ];
				# GPU names come from the PCI database, which NixOS keeps in the store.
				postFixup = ''
					wrapProgram $out/bin/elamond \
						--set-default ELAMON_PCI_IDS ${pkgs.hwdata}/share/hwdata/pci.ids
				'';
				postInstall = ''
					install -Dm644 dist/elamond.service $out/lib/systemd/user/elamond.service
					substituteInPlace $out/lib/systemd/user/elamond.service \
						--replace-fail /usr/bin/elamond $out/bin/elamond
				'';
				meta = {
					description = "The Elamon service: serves this computer's metrics to the Elamon app";
					license = pkgs.lib.licenses.gpl3Plus;
					mainProgram = "elamond";
					platforms = pkgs.lib.platforms.linux;
				};
			};

			defaultPackage = pkgs: self.packages.${pkgs.stdenv.hostPlatform.system}.elamond;

			# The settings a module writes, as `elamond check` reads them while they are built: a mistake
			# in a dashboard or an action fails `nixos-rebuild`, `home-manager switch` and `nix flake
			# check`, naming its line, rather than the service that would have read them.
			checkedSettings = pkgs: package: settings:
				let
					unchecked = (pkgs.formats.toml { }).generate "elamond-unchecked.toml" settings;
				in pkgs.runCommand "elamond.toml" { nativeBuildInputs = [ package ]; } ''
					mkdir -p "$TMPDIR/config" "$TMPDIR/data"
					HOME="$TMPDIR" ELAMON_CONFIG_DIR="$TMPDIR/config" ELAMON_DATA_DIR="$TMPDIR/data" \
						elamond check ${unchecked}
					cp ${unchecked} "$out"
				'';

			# What the settings option shows: a dashboard, an action with a parameter, a link and a
			# profile, as docs/config.md has them in TOML.
			settingsExample = lib: lib.literalExpression ''
				{
					control = true;
					links.jellyfin = { name = "Jellyfin"; url = "http://{host}:8096"; unit = "jellyfin.service"; scope = "system"; };
					actions.fan = {
						name = "Fan";
						command = [ "''${pkgs.coreutils}/bin/echo" "fan" "{speed}" ];
						params.speed = { choices = [ "quiet" "fast" ]; labels = { quiet = "Quiet"; fast = "Fast"; }; default = "quiet"; };
					};
					dashboards.power = {
						title = "Power";
						icon = "battery";
						widgets = [
							{ type = "gauge"; value = "battery.percent"; label = "Battery"; }
							{ type = "select"; action = "fan"; param = "speed"; label = "Fan"; }
						];
					};
					profiles.owner = { theme = "dark"; dashboards = [ "overview" "power" "processes" "control" ]; };
				}
			'';

			# The system units the settings' links name with `scope = "system"`: those phones may start,
			# stop and restart, which the polkit rule lets their users do.
			systemUnits = lib: settings:
				let
					links = settings.links or { };
				in
				lib.optionals (lib.isAttrs links) (
					lib.mapAttrsToList (_: l: l.unit) (lib.filterAttrs (_: l: lib.isAttrs l && l ? unit && (l.scope or "user") == "system") links)
				);

			# The Android SDK only exists for x86_64 hosts.
			dev = import nixpkgs {
				system = "x86_64-linux";
				config = {
					allowUnfree = true;
					android_sdk.accept_license = true;
				};
			};
			android = dev.androidenv.composeAndroidPackages {
				platformVersions = [ "37.0" ];
				buildToolsVersions = [ "37.0.0" ];
				includeEmulator = false;
				includeSystemImages = false;
				includeNDK = false;
			};
			sdk = "${android.androidsdk}/libexec/android-sdk";
		in {
			packages = forAll (pkgs: rec {
				elamond = elamondFor pkgs;
				default = elamond;
			});

			# NixOS: runs the Elamon service for every user who logs in, opens the firewall, and lets
			# chosen users act on the computer from the app without a password.
			#   services.elamond = {
			#     enable = true;
			#     openFirewall = true;
			#     settings.control = true;
			#     settings.links.jellyfin = { name = "Jellyfin"; url = "http://{host}:8096"; unit = "jellyfin.service"; scope = "system"; };
			#     control.users = [ "alice" ];
			#   };
			nixosModules.default = { config, lib, pkgs, ... }:
				let
					cfg = config.services.elamond;
					toml = pkgs.formats.toml { };
					configFile = checkedSettings pkgs cfg.package cfg.settings;
					units = lib.unique (cfg.control.units ++ systemUnits lib cfg.settings);
				in {
					options.services.elamond = {
						enable = lib.mkEnableOption "the Elamon service (elamond) as a user unit";
						package = lib.mkOption {
							type = lib.types.package;
							default = defaultPackage pkgs;
							defaultText = lib.literalExpression "elamon.packages.\${system}.elamond";
						};
						users = lib.mkOption {
							type = lib.types.listOf lib.types.str;
							default = [ ];
							example = [ "alice" ];
							description = ''
								Users whose sessions run the service. Empty: every regular user. System users
								(a display manager's greeter, for one) never do; they would take the port first.
								Only one user's service can hold the port at a time.
							'';
						};
						settings = lib.mkOption {
							type = toml.type;
							default = { };
							example = settingsExample lib;
							description = ''
								elamond.toml for every user's service, settings schema 2 as docs/config.md has it in
								TOML (`[dashboards.<id>]` is `dashboards.<id>`, `[[dashboards.<id>.widgets]]` a list
								of attribute sets); empty keeps each user's own ~/.config/elamon/elamond.toml. Each
								user's ~/.config/elamon/elamond.d/*.toml still adds to it. `elamond check` reads it as
								it is built, so a mistake fails the build with its line; `elamond defaults` prints the
								built-in dashboards to start from. A change restarts the services. It lands in the Nix
								store, which every user can read: keep secrets (a link's URL with a password in it, a
								token among an action's arguments) out of it, in a drop-in only its user can read
								(chmod 600).
							'';
						};
						settingsFile = lib.mkOption {
							type = lib.types.package;
							readOnly = true;
							internal = true;
							default = configFile;
							description = "`settings` as written and checked.";
						};
						openFirewall = lib.mkOption {
							type = lib.types.bool;
							default = false;
							description = "Open the service's TCP port (`settings.port`, 47823 by default) for the app and UDP 5353 so it finds the computer by mDNS (unless `settings.mdns = false`). Works without `enable`, for a service run by Home Manager.";
						};
						firewallInterfaces = lib.mkOption {
							type = lib.types.listOf lib.types.str;
							default = [ ];
							example = [ "eno1" "tailscale0" ];
							description = ''
								With `openFirewall`, open the ports on these interfaces only; empty opens them on
								every interface. UDP 5353 opens only on those whose names don't look like a VPN's
								(tailscale*, wg*, nordlynx*, tun*, tap*, zt*, ppp*): the service never announces itself
								over a VPN. That is told by the name alone, so a VPN interface named otherwise gets
								UDP 5353 too. To also listen only there, name them in `settings.listen`.
							'';
						};
						control = {
							users = lib.mkOption {
								type = lib.types.listOf lib.types.str;
								default = [ ];
								description = ''
									Users who may power off, reboot and suspend the computer and switch the power
									profile without a password, so that their Elamon service can: it runs outside a
									login session, where polkit would otherwise ask for one. polkit can't tell the
									service from the user's other programs, so this holds for every process these
									users run, not only elamond. Needs `control = true` in the service's settings to
									be of use.
								'';
							};
							units = lib.mkOption {
								type = lib.types.listOf lib.types.str;
								default = [ ];
								example = [ "jellyfin.service" ];
								description = ''
									System units those users may start, stop and restart without a password, from
									the app or any other program of theirs, beside those `settings.links` name with
									`scope = "system"`, which the rule has by itself. The app can only use a unit the
									service's settings name on a link (`[links.<id>]` with `unit` and
									`scope = "system"`): list here those of links in a user's drop-in.
								'';
							};
							multipleSessions = lib.mkOption {
								type = lib.types.bool;
								default = false;
								description = ''
									Also let those users power off, reboot and suspend while other users are logged in
									(logind's `*-multiple-sessions` actions), ending their sessions.
								'';
							};
						};
					};

					config = lib.mkMerge [
						(lib.mkIf cfg.enable {
							environment.systemPackages = [ cfg.package ];
							# `elamond pair`, `snapshot` and `clients` read the same settings as the service.
							environment.variables = lib.mkIf (cfg.settings != { }) { ELAMON_CONFIG = "${configFile}"; };
							systemd.user.services.elamond = {
								description = "Elamon service";
								wantedBy = [ "default.target" ];
								# Every user manager has this unit; these say whose may start it. "|" entries
								# need one of them to hold, the plain one always.
								unitConfig.ConditionUser = [ "!@system" ] ++ map (u: "|${u}") cfg.users;
								# NixOS gives a unit a PATH of coreutils and the like; the service runs wpctl or
								# pactl, and actions may run sudo, pkexec or what the user installed.
								enableDefaultPath = false;
								environment = {
									PATH = "/run/wrappers/bin:%h/.nix-profile/bin:/etc/profiles/per-user/%u/bin:/nix/var/nix/profiles/default/bin:/run/current-system/sw/bin";
								} // lib.optionalAttrs (cfg.settings != { }) { ELAMON_CONFIG = "${configFile}"; };
								restartTriggers = lib.optional (cfg.settings != { }) configFile;
								# A broken identity or setting fails at once; ten tries in five minutes, then stop.
								startLimitIntervalSec = 300;
								startLimitBurst = 10;
								serviceConfig = {
									ExecStart = "${cfg.package}/bin/elamond serve";
									ExecReload = "${pkgs.coreutils}/bin/kill -HUP $MAINPID";
									Restart = "on-failure";
									RestartSec = 5;
									# Room for its connections (256 at most) and the files it reads.
									LimitNOFILE = 8192;
								};
							};
						})
						(lib.mkIf cfg.openFirewall (
							let
								port = cfg.settings.port or 47823;
								mdns = cfg.settings.mdns or true;
								# Names VPN interfaces go by, where mDNS isn't opened: the service never announces
								# itself over a VPN. A guess by the name alone.
								vpn = name: lib.any (p: lib.hasPrefix p name) [ "tun" "tap" "wg" "nordlynx" "tailscale" "zt" "ppp" ];
								everywhere = cfg.firewallInterfaces == [ ];
							in {
								networking.firewall = {
									allowedTCPPorts = lib.optional everywhere port;
									allowedUDPPorts = lib.optional (everywhere && mdns) 5353;
									interfaces = lib.genAttrs cfg.firewallInterfaces (name: {
										allowedTCPPorts = [ port ];
										allowedUDPPorts = lib.optional (mdns && !vpn name) 5353;
									});
								};
							}
						))
						(lib.mkIf (cfg.control.users != [ ]) {
							security.polkit.enable = true;
							security.polkit.extraConfig = ''
								// Elamon: what paired phones may do through these users' Elamon services, and so
								// any program of these users without a password.
								polkit.addRule(function (action, subject) {
									if (${builtins.toJSON cfg.control.users}.indexOf(subject.user) < 0) {
										return polkit.Result.NOT_HANDLED;
									}
									var power = ${builtins.toJSON (
										[
											"org.freedesktop.login1.power-off"
											"org.freedesktop.login1.reboot"
											"org.freedesktop.login1.suspend"
											"org.freedesktop.UPower.PowerProfiles.switch-profile"
											"net.hadess.PowerProfiles.switch-profile"
										]
										++ lib.optionals cfg.control.multipleSessions [
											"org.freedesktop.login1.power-off-multiple-sessions"
											"org.freedesktop.login1.reboot-multiple-sessions"
											"org.freedesktop.login1.suspend-multiple-sessions"
										]
									)};
									if (power.indexOf(action.id) >= 0) {
										return polkit.Result.YES;
									}
									if (action.id == "org.freedesktop.systemd1.manage-units"
										&& ${builtins.toJSON units}.indexOf(action.lookup("unit")) >= 0
										&& ["start", "stop", "restart"].indexOf(action.lookup("verb")) >= 0) {
										return polkit.Result.YES;
									}
									return polkit.Result.NOT_HANDLED;
								});
							'';
						})
					];
				};

			# Home Manager: runs the Elamon service for this user.
			#   services.elamond = { enable = true; settings.control = true; };
			homeManagerModules.default = { config, lib, pkgs, ... }:
				let
					cfg = config.services.elamond;
					toml = pkgs.formats.toml { };
					configFile = checkedSettings pkgs cfg.package cfg.settings;
				in {
					options.services.elamond = {
						enable = lib.mkEnableOption "the Elamon service (elamond)";
						package = lib.mkOption {
							type = lib.types.package;
							default = defaultPackage pkgs;
							defaultText = lib.literalExpression "elamon.packages.\${system}.elamond";
						};
						settings = lib.mkOption {
							type = toml.type;
							default = { };
							example = settingsExample lib;
							description = ''
								Written to ~/.config/elamon/elamond.toml, settings schema 2 as docs/config.md has it in
								TOML; a change restarts the service; empty leaves that file alone. `elamond check`
								reads it as it is built, so a mistake fails `home-manager switch` with its line. It
								lands in the Nix store, which every user can read: keep secrets (a link's URL with a
								password in it, a token among an action's arguments) in a drop-in in
								~/.config/elamon/elamond.d only you can read (chmod 600).
							'';
						};
					};
					config = lib.mkIf cfg.enable {
						home.packages = [ cfg.package ];
						xdg.configFile."elamon/elamond.toml" = lib.mkIf (cfg.settings != { }) { source = configFile; };
						systemd.user.services.elamond = {
							Unit = {
								Description = "Elamon service";
								# A new elamond.toml restarts the service: port, interval and history only apply at start.
								X-Restart-Triggers = lib.optional (cfg.settings != { }) "${configFile}";
								StartLimitIntervalSec = 300;
								StartLimitBurst = 10;
							};
							Service = {
								ExecStart = "${cfg.package}/bin/elamond serve";
								ExecReload = "${pkgs.coreutils}/bin/kill -HUP $MAINPID";
								Restart = "on-failure";
								RestartSec = 5;
								LimitNOFILE = 8192;
							};
							Install.WantedBy = [ "default.target" ];
						};
					};
				};

			# The same under the name Home Manager now looks for.
			homeModules.default = self.homeManagerModules.default;

			# `nix flake check`: the NixOS module in a VM, as on a machine with a greeter and
			# more than one user; and settings with a mistake failing the build.
			checks.x86_64-linux.module = nixpkgs.legacyPackages.x86_64-linux.testers.runNixOSTest {
				name = "elamond-module";
				nodes.machine = { pkgs, ... }: {
					imports = [ self.nixosModules.default ];
					services.elamond = {
						enable = true;
						openFirewall = true;
						# The test reaches the service over loopback, which the firewall always lets through.
						firewallInterfaces = [ "eth1" "wg0" ];
						users = [ "alice" ];
						# A dashboard, an action with a parameter and a profile, in Nix.
						settings = {
							actions.greet = {
								name = "Greet";
								command = [ "${pkgs.coreutils}/bin/echo" "hello" "{who}" ];
								params.who = { choices = [ "world" "nix" ]; labels = { world = "World"; nix = "Nix"; }; };
								confirm = false;
							};
							dashboards.vm = {
								title = "VM";
								icon = "chip";
								widgets = [
									{ type = "value"; value = "cpu.usage"; label = "CPU"; }
									{ type = "select"; action = "greet"; param = "who"; label = "Greet"; }
									{ type = "table"; source = "links"; label = "Links"; columns = [ { field = "name"; } { field = "url"; } ]; }
								];
							};
							profiles.vm = { name = "VM"; dashboards = [ "vm" "overview" ]; };
						};
					};
					users.users.alice = { isNormalUser = true; uid = 1000; };
					users.users.bob = { isNormalUser = true; uid = 1001; };
					# A display manager's greeter: a system user with a user manager of its own.
					users.users.greeter = { isSystemUser = true; uid = 990; group = "greeter"; };
					users.groups.greeter = { };
					environment.systemPackages = [ pkgs.curl pkgs.jq pkgs.openssl ];
				};
				testScript = builtins.readFile ./nix/module-test.py;
			};

			# A widget with an address no computer has, in the module's settings: the settings fail to
			# build, `elamond check` naming the line and what was meant, and no service ever reads them.
			checks.x86_64-linux.settings-mistake =
				let
					pkgs = nixpkgs.legacyPackages.x86_64-linux;
					system = nixpkgs.lib.nixosSystem {
						system = "x86_64-linux";
						modules = [
							self.nixosModules.default
							{
								boot.isContainer = true;
								system.stateVersion = "26.05";
								services.elamond = {
									enable = true;
									settings.dashboards.broken = {
										title = "Broken";
										widgets = [ { type = "gauge"; value = "cpu.usge"; } ];
									};
								};
							}
						];
					};
					failed = pkgs.testers.testBuildFailure system.config.services.elamond.settingsFile;
				in
				pkgs.runCommand "elamond-settings-mistake" { } ''
					grep -F 'dashboards.broken.widgets[0].value: no value "cpu.usge"; did you mean cpu.usage?' ${failed}/testBuildFailure.log
					grep -qx 1 ${failed}/testBuildFailure.exit
					touch "$out"
				'';

			devShells.x86_64-linux = {
				default = dev.mkShell {
					packages = [
						android.androidsdk dev.jdk17 dev.gradle
						dev.cargo dev.rustc dev.clippy dev.rustfmt
					];
					ANDROID_HOME = sdk;
					ANDROID_SDK_ROOT = sdk;
					JAVA_HOME = dev.jdk17.home;
					# AGP fetches a dynamically linked aapt2 from Maven, which doesn't run on NixOS
					GRADLE_OPTS = "-Dorg.gradle.project.android.aapt2FromMavenOverride=${sdk}/build-tools/37.0.0/aapt2";
				};

				# `nix develop .#emulator`: an x86_64 emulator to try the app without a phone.
				emulator =
					let
						emu = dev.androidenv.composeAndroidPackages {
							platformVersions = [ "36" ];
							includeEmulator = true;
							includeSystemImages = true;
							systemImageTypes = [ "default" ];
							abiVersions = [ "x86_64" ];
						};
					in dev.mkShell {
						packages = [ emu.androidsdk ];
						ANDROID_HOME = "${emu.androidsdk}/libexec/android-sdk";
						ANDROID_SDK_ROOT = "${emu.androidsdk}/libexec/android-sdk";
					};
			};
		};
}
