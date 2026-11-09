{
	description = "Elacontrol — watch and control a computer from an Android phone";

	inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

	outputs = { self, nixpkgs, ... }:
		let
			system = "x86_64-linux";
			pkgs = import nixpkgs {
				inherit system;
				config = {
					allowUnfree = true;
					android_sdk.accept_license = true;
				};
			};
			android = pkgs.androidenv.composeAndroidPackages {
				platformVersions = [ "35" ];
				buildToolsVersions = [ "35.0.0" ];
				includeEmulator = false;
				includeSystemImages = false;
				includeNDK = false;
			};
			sdk = "${android.androidsdk}/libexec/android-sdk";
			cargoToml = builtins.fromTOML (builtins.readFile ./elamond/Cargo.toml);
		in {
			packages.${system} = rec {
				elacontrold = pkgs.rustPlatform.buildRustPackage {
					pname = "elacontrold";
					version = cargoToml.package.version;
					src = ./elamond;
					cargoLock.lockFile = ./elamond/Cargo.lock;
					nativeBuildInputs = [ pkgs.makeWrapper ];
					# GPU names come from the PCI database, which NixOS keeps in the store.
					postFixup = ''
						wrapProgram $out/bin/elacontrold \
							--set-default ELACONTROL_PCI_IDS ${pkgs.hwdata}/share/hwdata/pci.ids
					'';
					postInstall = ''
						install -Dm644 dist/elacontrold.service $out/lib/systemd/user/elacontrold.service
						substituteInPlace $out/lib/systemd/user/elacontrold.service \
							--replace-fail /usr/bin/elacontrold $out/bin/elacontrold
					'';
					meta = {
						description = "Service that serves this computer's metrics to the Elacontrol app";
						license = pkgs.lib.licenses.gpl3Plus;
						mainProgram = "elacontrold";
						platforms = pkgs.lib.platforms.linux;
					};
				};
				default = elacontrold;
			};

			# Runs the service as a user service: `services.elacontrold.enable = true;`
			homeManagerModules.default = { config, lib, ... }:
				let cfg = config.services.elacontrold;
				in {
					options.services.elacontrold = {
						enable = lib.mkEnableOption "the Elacontrol service";
						package = lib.mkOption {
							type = lib.types.package;
							default = self.packages.${system}.elacontrold;
						};
					};
					config = lib.mkIf cfg.enable {
						home.packages = [ cfg.package ];
						systemd.user.services.elacontrold = {
							Unit.Description = "Elacontrol service";
							Service = {
								ExecStart = "${cfg.package}/bin/elacontrold serve";
								Restart = "on-failure";
								RestartSec = 5;
							};
							Install.WantedBy = [ "default.target" ];
						};
					};
				};

			devShells.${system}.default = pkgs.mkShell {
				packages = [
					android.androidsdk pkgs.jdk17 pkgs.gradle
					pkgs.cargo pkgs.rustc pkgs.clippy pkgs.rustfmt
				];
				ANDROID_HOME = sdk;
				ANDROID_SDK_ROOT = sdk;
				JAVA_HOME = pkgs.jdk17.home;
				# AGP fetches a dynamically linked aapt2 from Maven, which doesn't run on NixOS
				GRADLE_OPTS = "-Dorg.gradle.project.android.aapt2FromMavenOverride=${sdk}/build-tools/35.0.0/aapt2";
			};
		};
}
