{
	description = "Elamon — watch and control a computer from an Android phone";

	inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

	outputs = { self, nixpkgs, ... }:
		let
			# The service runs on PCs and on ARM boards alike.
			systems = [ "x86_64-linux" "aarch64-linux" ];
			forAll = f: nixpkgs.lib.genAttrs systems (s: f nixpkgs.legacyPackages.${s});
			cargoToml = builtins.fromTOML (builtins.readFile ./elamond/Cargo.toml);

			elamondFor = pkgs: pkgs.rustPlatform.buildRustPackage {
				pname = "elamond";
				version = cargoToml.package.version;
				# Its own store path, so commits that touch only the app leave the service as built.
				src = builtins.path {
					path = ./elamond;
					name = "elamond-source";
				};
				cargoLock.lockFile = ./elamond/Cargo.lock;
				nativeBuildInputs = [ pkgs.makeWrapper ];
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
					description = "Service that serves this computer's metrics to the Elamon app";
					license = pkgs.lib.licenses.gpl3Plus;
					mainProgram = "elamond";
					platforms = pkgs.lib.platforms.linux;
				};
			};

			defaultPackage = pkgs: self.packages.${pkgs.stdenv.hostPlatform.system}.elamond;

			# The Android SDK only exists for x86_64 hosts.
			dev = import nixpkgs {
				system = "x86_64-linux";
				config = {
					allowUnfree = true;
					android_sdk.accept_license = true;
				};
			};
			android = dev.androidenv.composeAndroidPackages {
				platformVersions = [ "35" ];
				buildToolsVersions = [ "35.0.0" ];
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

			# NixOS: runs the service for every user who logs in, opens the firewall, and lets
			# chosen users act on the computer from the app without a password.
			#   services.elamond = {
			#     enable = true;
			#     openFirewall = true;
			#     settings.control = true;
			#     control.users = [ "alice" ];
			#     control.units = [ "jellyfin.service" ];
			#   };
			nixosModules.default = { config, lib, pkgs, ... }:
				let
					cfg = config.services.elamond;
					toml = pkgs.formats.toml { };
					configFile = toml.generate "elamond.toml" cfg.settings;
				in {
					options.services.elamond = {
						enable = lib.mkEnableOption "the Elamon service, as a user service";
						package = lib.mkOption {
							type = lib.types.package;
							default = defaultPackage pkgs;
							defaultText = lib.literalExpression "elamon.packages.\${system}.elamond";
						};
						settings = lib.mkOption {
							type = toml.type;
							default = { };
							description = "elamond.toml for every user's service; empty keeps each user's own ~/.config/elamon/elamond.toml.";
						};
						openFirewall = lib.mkOption {
							type = lib.types.bool;
							default = false;
							description = "Open TCP 47823 for the app and UDP 5353 so it finds the computer by mDNS. Works without `enable`, for services run by Home Manager.";
						};
						control = {
							users = lib.mkOption {
								type = lib.types.listOf lib.types.str;
								default = [ ];
								description = ''
									Users whose service may power off, reboot and suspend the computer and switch
									the power profile without a password. Their service runs outside a login
									session, where polkit would otherwise ask for one. Needs `control = true`
									in the service's settings to be of use.
								'';
							};
							units = lib.mkOption {
								type = lib.types.listOf lib.types.str;
								default = [ ];
								example = [ "jellyfin.service" ];
								description = "System units those users may start, stop and restart from the app.";
							};
						};
					};

					config = lib.mkMerge [
						(lib.mkIf cfg.enable {
							environment.systemPackages = [ cfg.package ];
							systemd.user.services.elamond = {
								description = "Elamon service";
								wantedBy = [ "default.target" ];
								environment = lib.mkIf (cfg.settings != { }) { ELAMON_CONFIG = "${configFile}"; };
								restartTriggers = lib.optional (cfg.settings != { }) configFile;
								serviceConfig = {
									ExecStart = "${cfg.package}/bin/elamond serve";
									Restart = "on-failure";
									RestartSec = 5;
								};
							};
						})
						(lib.mkIf cfg.openFirewall {
							networking.firewall = {
								allowedTCPPorts = [ 47823 ];
								allowedUDPPorts = [ 5353 ];
							};
						})
						(lib.mkIf (cfg.control.users != [ ]) {
							security.polkit.enable = true;
							security.polkit.extraConfig = ''
								// Elamon: what paired phones may do through these users' services.
								polkit.addRule(function (action, subject) {
									if (${builtins.toJSON cfg.control.users}.indexOf(subject.user) < 0) {
										return polkit.Result.NOT_HANDLED;
									}
									var power = [
										"org.freedesktop.login1.power-off",
										"org.freedesktop.login1.power-off-multiple-sessions",
										"org.freedesktop.login1.reboot",
										"org.freedesktop.login1.reboot-multiple-sessions",
										"org.freedesktop.login1.suspend",
										"org.freedesktop.login1.suspend-multiple-sessions",
										"org.freedesktop.UPower.PowerProfiles.switch-profile",
										"net.hadess.PowerProfiles.switch-profile"
									];
									if (power.indexOf(action.id) >= 0) {
										return polkit.Result.YES;
									}
									if (action.id == "org.freedesktop.systemd1.manage-units"
										&& ${builtins.toJSON cfg.control.units}.indexOf(action.lookup("unit")) >= 0
										&& ["start", "stop", "restart"].indexOf(action.lookup("verb")) >= 0) {
										return polkit.Result.YES;
									}
									return polkit.Result.NOT_HANDLED;
								});
							'';
						})
					];
				};

			# Home Manager: runs the service for this user.
			#   services.elamond = { enable = true; settings.control = true; };
			homeManagerModules.default = { config, lib, pkgs, ... }:
				let
					cfg = config.services.elamond;
					toml = pkgs.formats.toml { };
					configFile = toml.generate "elamond.toml" cfg.settings;
				in {
					options.services.elamond = {
						enable = lib.mkEnableOption "the Elamon service";
						package = lib.mkOption {
							type = lib.types.package;
							default = defaultPackage pkgs;
							defaultText = lib.literalExpression "elamon.packages.\${system}.elamond";
						};
						settings = lib.mkOption {
							type = toml.type;
							default = { };
							example = lib.literalExpression ''
								{
									control = true;
									services = [ { name = "Jellyfin"; url = "http://{host}:8096"; unit = "jellyfin.service"; scope = "system"; } ];
									actions = [ { name = "Update"; command = [ "nh" "os" "switch" ]; timeout = 900; } ];
								}
							'';
							description = "Written to ~/.config/elamon/elamond.toml; empty leaves that file alone.";
						};
					};
					config = lib.mkIf cfg.enable {
						home.packages = [ cfg.package ];
						xdg.configFile."elamon/elamond.toml" = lib.mkIf (cfg.settings != { }) { source = configFile; };
						systemd.user.services.elamond = {
							Unit = {
								Description = "Elamon service";
								# A new elamond.toml restarts the service, which reads it only at start.
								X-Restart-Triggers = lib.optional (cfg.settings != { }) "${configFile}";
							};
							Service = {
								ExecStart = "${cfg.package}/bin/elamond serve";
								Restart = "on-failure";
								RestartSec = 5;
							};
							Install.WantedBy = [ "default.target" ];
						};
					};
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
					GRADLE_OPTS = "-Dorg.gradle.project.android.aapt2FromMavenOverride=${sdk}/build-tools/35.0.0/aapt2";
				};

				# `nix develop .#emulator`: an x86_64 emulator to try the app without a phone.
				emulator =
					let
						emu = dev.androidenv.composeAndroidPackages {
							platformVersions = [ "35" ];
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
