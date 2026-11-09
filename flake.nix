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
				src = ./elamond;
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

			# NixOS: runs the service for every user who logs in.
			#   services.elamond = { enable = true; openFirewall = true; };
			nixosModules.default = { config, lib, pkgs, ... }:
				let cfg = config.services.elamond;
				in {
					options.services.elamond = {
						enable = lib.mkEnableOption "the Elamon service, as a user service";
						package = lib.mkOption {
							type = lib.types.package;
							default = defaultPackage pkgs;
							defaultText = lib.literalExpression "elamon.packages.\${system}.elamond";
						};
						openFirewall = lib.mkOption {
							type = lib.types.bool;
							default = false;
							description = "Open TCP 47823 for the app and UDP 5353 so it finds the computer by mDNS.";
						};
					};
					config = lib.mkIf cfg.enable {
						environment.systemPackages = [ cfg.package ];
						systemd.user.services.elamond = {
							description = "Elamon service";
							wantedBy = [ "default.target" ];
							serviceConfig = {
								ExecStart = "${cfg.package}/bin/elamond serve";
								Restart = "on-failure";
								RestartSec = 5;
							};
						};
						networking.firewall = lib.mkIf cfg.openFirewall {
							allowedTCPPorts = [ 47823 ];
							allowedUDPPorts = [ 5353 ];
						};
					};
				};

			# Home Manager: runs the service for this user.  services.elamond.enable = true;
			homeManagerModules.default = { config, lib, pkgs, ... }:
				let cfg = config.services.elamond;
				in {
					options.services.elamond = {
						enable = lib.mkEnableOption "the Elamon service";
						package = lib.mkOption {
							type = lib.types.package;
							default = defaultPackage pkgs;
							defaultText = lib.literalExpression "elamon.packages.\${system}.elamond";
						};
					};
					config = lib.mkIf cfg.enable {
						home.packages = [ cfg.package ];
						systemd.user.services.elamond = {
							Unit.Description = "Elamon service";
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
