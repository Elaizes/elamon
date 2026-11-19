{
	description = "Elamon — watch and control a computer from an Android phone";

	inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
	# nutflow, a NUT server for EcoFlow stations, serves a mock station to the UPS VM test
	# (checks.x86_64-linux.ups) and nothing else: neither the package nor the modules use it, so
	# a flake that takes elamon never fetches it to build.
	inputs.nutflow = {
		url = "github:Dolfost/nutflow";
		inputs.nixpkgs.follows = "nixpkgs";
	};

	outputs = { self, nixpkgs, nutflow, ... }:
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
				cargoLock = {
					lockFile = ./elamond/Cargo.lock;
					# upst-core comes from its repository, at a tag; this pins what it holds.
					outputHashes."upst-core-0.1.0" = "sha256-mI33ILxyCXIL3UsCJ92Cw7GEKbUPgaJhrWTxKrFfTwQ=";
				};
				nativeBuildInputs = [ pkgs.makeWrapper ];
				# `elamond update`'s tests download, with curl, from a server of their own on 127.0.0.1.
				nativeCheckInputs = [ pkgs.curl ];
				# GPU names come from the PCI database, which NixOS keeps in the store; a web service a
				# source reads is asked through curl, which a system without it in its PATH gets from here.
				postFixup = ''
					wrapProgram $out/bin/elamond \
						--set-default ELAMON_PCI_IDS ${pkgs.hwdata}/share/hwdata/pci.ids \
						--suffix PATH : ${pkgs.curl}/bin
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

			# The settings elamond starts with (the starter elamond.toml, elamond.d/dashboards.toml and
			# elamond.d/events.toml; defaults.toml), as Nix reads them: three files of top-level keys apart.
			defaultSettings = builtins.fromTOML (builtins.readFile ./elamond/src/starter.toml)
				// builtins.fromTOML (builtins.readFile ./elamond/src/dashboards.toml)
				// builtins.fromTOML (builtins.readFile ./elamond/src/events.toml);

			# What the modules put in `settings` themselves: the defaults, each value (a list whole) at
			# mkDefault, so what is set there overrides them key by key, a list replacing theirs whole;
			# and `schema = 3`, a whole file.
			settingsDefaults = lib: lib.mapAttrsRecursive (_: lib.mkDefault) defaultSettings // { schema = 3; };

			# The read-only option that shows the defaults as they are.
			defaultsOption = lib: toml: lib.mkOption {
				type = toml.type;
				readOnly = true;
				default = defaultSettings;
				defaultText = lib.literalMD "the settings elamond starts with: `elamond defaults` prints them";
				description = ''
					The settings elamond starts with, which `settings` holds at mkDefault: the standard
					dashboards, the `default` profile, the templates and the top-level keys, as
					`elamond defaults` prints them. For reading and copying from.
				'';
			};

			# ddcutil where the service finds it, for an external monitor's brightness.
			ddcutilOption = lib: lib.mkOption {
				type = lib.types.bool;
				default = false;
				description = ''
					Give the service ddcutil, which it sets an external monitor's brightness with (DDC/CI) when
					the monitors are acted on under Hyprland. The user also needs the monitors' I2C devices:
					on NixOS `hardware.i2c.enable = true` and the user in its group (`i2c`); `elamond check`
					says what is missing. A laptop's panel needs none of this: logind sets it.
				'';
			};

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
			# profile, as docs/config.md has them in TOML, and one key of a standard dashboard changed.
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
					profiles.owner = { use = "default"; theme = "dark"; dashboards = [ "overview" "power" "processes" "control" ]; };
					dashboards.overview.title = "Home";
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
								elamond.toml for every user's service, settings schema 3 as docs/config.md has it in
								TOML (`[dashboards.<id>]` is `dashboards.<id>`, `[[dashboards.<id>.widgets]]` a list
								of attribute sets). It holds the settings elamond starts with (`defaults`) at
								mkDefault, and `schema = 3`: what you set overrides them key by key, a list (a
								dashboard's `widgets`) replacing theirs whole, and a table of your own adds to them;
								`lib.mkForce` on a table (`dashboards`) leaves theirs out. The services read it
								through ELAMON_CONFIG; a user's own ~/.config/elamon/elamond.toml, where there is one, after it
								then each user's ~/.config/elamon/elamond.d/*.toml,
								phones.toml among them, where each phone paired has a profile of its own. `elamond check` reads it as it is
								built, so a mistake fails the build with its line. A change restarts the services. It
								lands in the Nix store, which every user can read: keep secrets (a link's URL with a
								password in it, a token among an action's arguments) out of it, in a drop-in only its
								user can read (chmod 600).
							'';
						};
						ddcutil = ddcutilOption lib;
						defaults = defaultsOption lib toml;
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
						{ services.elamond.settings = settingsDefaults lib; }
						(lib.mkIf cfg.enable {
							environment.systemPackages = [ cfg.package ];
							# `elamond pair`, `snapshot` and `clients` read the same settings as the service.
							environment.variables.ELAMON_CONFIG = "${configFile}";
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
									PATH = lib.optionalString cfg.ddcutil "${pkgs.ddcutil}/bin:" + "/run/wrappers/bin:%h/.nix-profile/bin:/etc/profiles/per-user/%u/bin:/nix/var/nix/profiles/default/bin:/run/current-system/sw/bin";
									ELAMON_CONFIG = "${configFile}";
								};
								restartTriggers = [ configFile ];
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
								Written to ~/.config/elamon/elamond.toml, settings schema 3 as docs/config.md has it in
								TOML. It holds the settings elamond starts with (`defaults`) at mkDefault, and
								`schema = 3`: what you set overrides them key by key, a list (a dashboard's
								`widgets`) replacing theirs whole, and a table of your own adds to them;
								`lib.mkForce` on a table (`dashboards`) leaves theirs out. An elamond.toml of your own
								there is in the way: move what is yours into a drop-in, ~/.config/elamon/elamond.d/*.toml,
								which adds to it as phones.toml does (each phone paired has a profile of its own
								there). A change restarts the service. `elamond check` reads it as it is built, so a
								mistake fails `home-manager switch` with its line. It lands in the Nix store, which
								every user can read: keep secrets (a link's URL with a password in it, a token among
								an action's arguments) in a drop-in only you can read (chmod 600).
							'';
						};
						defaults = defaultsOption lib toml;
						ddcutil = ddcutilOption lib;
					};
					config = lib.mkMerge [
						{ services.elamond.settings = settingsDefaults lib; }
						(lib.mkIf cfg.enable {
							# ddcutil among the user's packages, where the service finds programs.
							home.packages = [ cfg.package ] ++ lib.optional cfg.ddcutil pkgs.ddcutil;
							xdg.configFile."elamon/elamond.toml".source = configFile;
							systemd.user.services.elamond = {
								Unit = {
									Description = "Elamon service";
									# A new elamond.toml restarts the service: port, interval and history only apply at start.
									X-Restart-Triggers = [ "${configFile}" ];
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
						})
					];
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
						# A dashboard, an action with a parameter and a profile, in Nix, beside the defaults
						# the module adds.
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

			# elamond with a UPS source on nutflow's mock station (a DELTA 3 Max Plus it simulates, no
			# Bluetooth), on one machine: the station's values in ui and the stream; its outlets switched
			# and its charge limit set through the API, as the station says; what the source doesn't
			# allow refused; the history across a restart; and the NUT password nowhere elamond writes.
			checks.x86_64-linux.ups = nixpkgs.legacyPackages.x86_64-linux.testers.runNixOSTest {
				name = "elamond-ups";
				nodes.machine = { pkgs, ... }: {
					imports = [ self.nixosModules.default nutflow.nixosModules.default ];
					# The NUT user's password, made at boot and so never in the Nix store: root's alone for
					# nutflow, which reads it through LoadCredential=, and alice's copy for her service.
					systemd.services.nut-password = {
						wantedBy = [ "multi-user.target" ];
						requiredBy = [ "nutflow.service" ];
						before = [ "nutflow.service" "user@1000.service" ];
						path = [ pkgs.coreutils ];
						serviceConfig = { Type = "oneshot"; RemainAfterExit = true; };
						script = ''
							umask 077
							mkdir -p /var/lib/secrets
							test -s /var/lib/secrets/nut-elamon || echo "nut-$(head -c 15 /dev/urandom | base32)" > /var/lib/secrets/nut-elamon
							install -d -o alice -g users -m 700 /home/alice/.config /home/alice/.config/elamon
							install -o alice -g users -m 600 /var/lib/secrets/nut-elamon /home/alice/.config/elamon/nut.password
						'';
					};
					services.nutflow = {
						enable = true;
						# nutflow's own tests run in its own checks; built here without them.
						package = nutflow.packages.x86_64-linux.default.overrideAttrs { doCheck = false; };
						stations = [ { name = "ups"; kind = "mock"; description = "Mock DELTA 3 Max Plus"; } ];
						# What elamond's source may run, but the beeper: one the station refuses.
						users.elamon = {
							passwordFile = "/var/lib/secrets/nut-elamon";
							allow = [ "outlet.1.load.*" "outlet.2.load.*" "experimental.battery.charge.max" ];
						};
					};
					services.elamond = {
						enable = true;
						users = [ "alice" ];
						settings = {
							control = true;
							sources.eco = {
								kind = "nut";
								ups = "ups";
								capacity_wh = 2048;
								user = "elamon";
								password_file = "/home/alice/.config/elamon/nut.password";
								# The station has a switch for each of its nine outlets: those past 2 aren't listed.
								commands = [ "outlet.1.load.*" "outlet.2.load.*" "beeper.*" ];
								settings = [ "experimental.battery.charge.max" ];
								outlets."1".critical = true;
							};
						};
					};
					users.users.alice = { isNormalUser = true; uid = 1000; linger = true; };
					environment.systemPackages = [ pkgs.curl pkgs.jq pkgs.nut ];
				};
				testScript = builtins.readFile ./nix/ups-test.py;
			};

			# elamond with sources of its own and the journal, on one machine: a command's, a file's and a
			# web service's values, read only while a phone looks; the web service's token in its header
			# and nowhere elamond writes or answers; the user's own journal, then, in systemd-journal, the
			# system's, live and in pages, a unit's from its link; journalctl gone with the last stream.
			checks.x86_64-linux.journal = nixpkgs.legacyPackages.x86_64-linux.testers.runNixOSTest {
				name = "elamond-journal";
				nodes.machine = { pkgs, ... }: {
					imports = [ self.nixosModules.default ];
					# A web service on loopback that wants a token, and writes down each Authorization it gets.
					systemd.services.token-echo = {
						wantedBy = [ "multi-user.target" ];
						serviceConfig = {
							StateDirectory = "token-echo";
							ExecStart = pkgs.writers.writePython3 "token-echo" { flakeIgnore = [ "E501" ]; } (builtins.readFile ./nix/token-echo.py);
						};
					};
					# The token, made at boot and so never in the Nix store: alice's alone.
					systemd.services.app-token = {
						wantedBy = [ "multi-user.target" ];
						before = [ "user@1000.service" ];
						path = [ pkgs.coreutils ];
						serviceConfig = { Type = "oneshot"; RemainAfterExit = true; };
						script = ''
							install -d -o alice -g users -m 700 /home/alice/.config /home/alice/.config/elamon
							test -s /home/alice/.config/elamon/app.token || echo "tok-$(head -c 15 /dev/urandom | base32)" > /home/alice/.config/elamon/app.token
							chown alice:users /home/alice/.config/elamon/app.token
							chmod 600 /home/alice/.config/elamon/app.token
						'';
					};
					services.elamond = {
						enable = true;
						users = [ "alice" ];
						settings = {
							control = true;
							sources.up = { kind = "command"; command = [ "${pkgs.coreutils}/bin/printf" ''{"load": 0.5}'' ]; interval = 2; };
							sources.mark = { kind = "file"; path = "/home/alice/mark"; format = "text"; interval = 1; };
							sources.app = { kind = "http"; url = "http://127.0.0.1:8080/status"; token_file = "/home/alice/.config/elamon/app.token"; interval = 2; };
							links.echo = { name = "Echo"; url = "http://{host}:8080"; unit = "token-echo.service"; scope = "system"; };
							dashboards.mine = {
								title = "Mine";
								icon = "terminal";
								widgets = [
									{ id = "load"; type = "value"; value = "up.load"; label = "Load"; }
									{ id = "mark"; type = "value"; value = "mark.text"; label = "Mark"; }
									{ id = "app"; type = "status"; value = "app.state"; label = "App"; }
									{ id = "queue"; type = "value"; value = "app.queue.length"; label = "Queue"; }
								];
							};
							profiles.vm = { name = "VM"; dashboards = [ "mine" "control" ]; };
						};
					};
					users.users.alice = { isNormalUser = true; uid = 1000; linger = true; };
					environment.systemPackages = [ pkgs.curl pkgs.jq pkgs.procps pkgs.util-linux ];
				};
				testScript = builtins.readFile ./nix/journal-test.py;
			};

			# elamond's events on one machine, sent to a mock ntfy on loopback that writes down each
			# request: a disk filling and emptied, across a restart; a link's unit failing; `elamond
			# notify`; a UPS on battery and back (NUT's dummy-ups, whose file the test rewrites);
			# rules changed and turned off by a drop-in; what a target without names sends; and the
			# topics and the token nowhere but their files and the requests.
			checks.x86_64-linux.events = nixpkgs.legacyPackages.x86_64-linux.testers.runNixOSTest {
				name = "elamond-events";
				nodes.machine = { pkgs, ... }: {
					imports = [ self.nixosModules.default ];
					# A name of its own, looked for in what a target without names sends.
					networking.hostName = "evbench";
					systemd.services.ntfy-mock = {
						wantedBy = [ "multi-user.target" ];
						serviceConfig = {
							StateDirectory = "ntfy-mock";
							ExecStart = pkgs.writers.writePython3 "ntfy-mock" { flakeIgnore = [ "E501" ]; } (builtins.readFile ./nix/ntfy-mock.py);
						};
					};
					# Two topics and a token, made at boot and so never in the Nix store: alice's alone.
					systemd.services.ntfy-secrets = {
						wantedBy = [ "multi-user.target" ];
						before = [ "user@1000.service" ];
						path = [ pkgs.coreutils ];
						serviceConfig = { Type = "oneshot"; RemainAfterExit = true; };
						script = ''
							umask 077
							install -d -o alice -g users -m 700 /home/alice/.config /home/alice/.config/elamon
							cd /home/alice/.config/elamon
							test -s ntfy.topic || echo "topic-marker-$(head -c 10 /dev/urandom | base32 | tr -d =)" > ntfy.topic
							test -s ntfy-named.topic || echo "topic-named-$(head -c 10 /dev/urandom | base32 | tr -d =)" > ntfy-named.topic
							test -s ntfy.token || echo "tk_$(head -c 15 /dev/urandom | base32)" > ntfy.token
							chown alice:users ntfy.topic ntfy-named.topic ntfy.token
							chmod 600 ntfy.topic ntfy-named.topic ntfy.token
						'';
					};
					# A small ext4 the service lists as a disk (it leaves tmpfs out), there before it starts.
					systemd.services.scratch-disk = {
						wantedBy = [ "multi-user.target" ];
						before = [ "user@1000.service" ];
						path = [ pkgs.coreutils pkgs.e2fsprogs pkgs.util-linux ];
						serviceConfig = { Type = "oneshot"; RemainAfterExit = true; };
						script = ''
							mkdir -p /var/lib/scratch /srv/scratch
							test -e /var/lib/scratch/disk.img || { truncate -s 32M /var/lib/scratch/disk.img && mkfs.ext4 -q -F -m 0 /var/lib/scratch/disk.img; }
							mountpoint -q /srv/scratch || mount -o loop /var/lib/scratch/disk.img /srv/scratch
						'';
					};
					# A link's unit, which fails as it starts.
					systemd.services.ledger-sync = {
						description = "A unit that fails";
						serviceConfig.ExecStart = "${pkgs.coreutils}/bin/false";
					};
					# A UPS on upsd that NUT's dummy-ups serves from a file, which the test rewrites to take
					# it on battery and back: nutflow's mock station fails its grid on a schedule of its own.
					systemd.services.cellar-ups-file = {
						wantedBy = [ "multi-user.target" ];
						before = [ "upsdrv.service" "upsd.service" ];
						path = [ pkgs.coreutils ];
						serviceConfig = { Type = "oneshot"; RemainAfterExit = true; };
						script = ''
							mkdir -p /var/lib/cellar-ups
							printf '%s\n' 'device.mfr: Example' 'device.model: Bench 1500' 'ups.mfr: Example' 'ups.model: Bench 1500' \
								'ups.status: OL' 'battery.charge: 100' 'battery.runtime: 3600' 'ups.load: 20' > /var/lib/cellar-ups/cellarups.dev
						'';
					};
					power.ups = {
						enable = true;
						mode = "standalone";
						# Nothing shuts the machine down for what the UPS says.
						upsmon.enable = false;
						upsd.listen = [ { address = "127.0.0.1"; } ];
						ups.cellarups = {
							driver = "dummy-ups";
							port = "/var/lib/cellar-ups/cellarups.dev";
							description = "A UPS the test switches";
						};
					};
					systemd.services.upsd.before = [ "user@1000.service" ];
					services.elamond = {
						enable = true;
						users = [ "alice" ];
						settings = {
							# Links' units read every 5 seconds while no phone looks; the disk rule after 3.
							events.background = 5;
							events.notify.length = 60;
							# Room for every request the test makes, for each target ([events.target]'s).
							events.target.burst = 100;
							rules.disk.for = 3;
							rules.boot.enabled = true;
							notify.mock = {
								url = "http://127.0.0.1:8090";
								topic_file = "/home/alice/.config/elamon/ntfy.topic";
							};
							notify.named = {
								url = "http://127.0.0.1:8090";
								topic_file = "/home/alice/.config/elamon/ntfy-named.topic";
								token_file = "/home/alice/.config/elamon/ntfy.token";
								names = true;
							};
							links.ledger = { name = "Ledger"; url = "http://{host}:8099"; unit = "ledger-sync.service"; scope = "system"; };
							sources.cellar = { kind = "nut"; ups = "cellarups"; host = "127.0.0.1"; background_ms = 1000; };
						};
					};
					users.users.alice = { isNormalUser = true; uid = 1000; linger = true; };
					environment.systemPackages = [ pkgs.curl pkgs.jq pkgs.procps pkgs.util-linux ];
				};
				testScript = builtins.readFile ./nix/events-test.py;
			};

			# elamond and the monitors under Hyprland, on one machine: a Hyprland of its own on Lua
			# settings, drawn by Mesa's software renderer on QEMU's virtio GPU (its screen Virtual-1), and
			# two outputs without a screen, found by the service by itself, turned off and on and put to
			# sleep through the API as Hyprland then says; the last screen off only with its name typed.
			# Then elamond's own stand-in hyprctl with a laptop's panel off, a monitor and a stream's
			# output: what a phone is offered, may and may not do, the activity log, and ddcutil where
			# the module's option puts it.
			checks.x86_64-linux.displays = nixpkgs.legacyPackages.x86_64-linux.testers.runNixOSTest {
				name = "elamond-displays";
				nodes.machine = { pkgs, ... }: {
					imports = [ self.nixosModules.default ];
					# A GPU with a screen, which Mesa draws on in software, as Hyprland's own VM test has it.
					virtualisation.qemu.options = [ "-vga none" "-device virtio-gpu-pci" ];
					virtualisation.memorySize = 2048;
					virtualisation.cores = 2;
					hardware.graphics.enable = true;
					# alice logged in on tty1: the seat's session, which Hyprland started from her user
					# manager takes the screen through.
					services.getty.autologinUser = "alice";
					# Each output in a place of its own, so that one turned off and on again leaves the
					# others where they were (Hyprland moves those placed `auto`).
					environment.etc."hypr/elamon-test.lua".text = ''
						hl.monitor({ output = "Virtual-1", mode = "preferred", position = "0x0", scale = 1 })
						hl.monitor({ output = "A", mode = "1920x1080@60", position = "1280x0", scale = 1 })
						hl.monitor({ output = "B", mode = "1920x1080@60", position = "3200x0", scale = 1 })
						hl.monitor({ output = "", mode = "preferred", position = "auto", scale = 1 })
						hl.config({
							animations = { enabled = false },
							misc = { disable_hyprland_logo = true, disable_splash_rendering = true, force_default_wallpaper = 0 },
						})
					'';
					# The stand-in hyprctl of elamond's own tests, which keeps its state beside itself: the test
					# copies it into a folder of alice's, ahead of Hyprland's on the service's PATH.
					environment.etc."elamon-test/hyprctl-stand-in".source = ./elamond/src/displays/hyprctl-stand-in.sh;
					services.elamond = {
						enable = true;
						users = [ "alice" ];
						ddcutil = true;
						settings = {
							control = true;
							# The outputs without a screen too: the real Hyprland here has two.
							displays.headless = true;
							displays.every = 1;
						};
					};
					users.users.alice = { isNormalUser = true; uid = 1000; linger = true; };
					environment.systemPackages = [ pkgs.curl pkgs.jq pkgs.hyprland ];
				};
				testScript = builtins.readFile ./nix/displays-test.py;
			};

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
