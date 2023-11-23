{
  inputs = {
    nixpkgs.url = "nixpkgs/nixos-unstable";
    flake-parts.url = "github:hercules-ci/flake-parts";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    crane = {
      url = "github:ipetkov/crane";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = inputs@{ self, flake-parts, rust-overlay, crane, nixpkgs, ... }:
    flake-parts.lib.mkFlake { inherit inputs; } {
      systems = [
        "x86_64-linux"
        "aarch64-linux"
      ];

      perSystem = { pkgs, system, ... }:
        let
          pkgs = import nixpkgs {
            inherit system;
            overlays = [ (import rust-overlay) ];
          };

          libPath = with pkgs; [
            mold
            clang
            wayland
            dbus
            seatd
            libxkbcommon
            libinput
            systemd
            mesa
            libGL
            xwayland

            xorg.libXcursor
            xorg.libX11
            xorg.libXi
            xorg.libX11
            xorg.libXft
            xorg.libXrandr
            xorg.libXinerama
          ];

          commonArgs = {
            src = craneLib.cleanCargoSource (craneLib.path ./.);

            buildInputs = with pkgs; [
              pkg-config
              mold
              clang
              wayland
              dbus
              seatd
              libxkbcommon
              libinput
              mesa

              xorg.libX11
              xorg.libXft
              xorg.libXrandr
              xorg.libXinerama
            ];
          } // (craneLib.crateNameFromCargoToml { cargoToml = ./Cargo.toml; });

          craneLib = (crane.mkLib pkgs).overrideToolchain pkgs.rust-bin.stable.latest.minimal;

          cargoArtifacts = craneLib.buildDepsOnly (commonArgs // {
            panme = "buddaraysh-deps";
          });

          buddaraysh = craneLib.buildPackage (commonArgs // {
            src = craneLib.path ./.;
            inherit cargoArtifacts;

            postInstall = ''
              install -Dm0644 -t $out/share/wayland-sessions $src/buddaraysh.desktop
            '';

            postFixup = ''
                patchelf --set-rpath "${pkgs.lib.makeLibraryPath libPath}" $out/bin/buddaraysh
            '';

            NIX_CFLAGS_LINK = "-fuse-ld=mold";
          });

        in
        {

          # `nix build`
          packages = {
            inherit buddaraysh;
            default = buddaraysh;
          };
          
          # `nix develop`
          devShells.default = pkgs.mkShell
            {
              NIX_CFLAGS_LINK = "-fuse-ld=mold";

              packages = with pkgs; [
                (rust-bin.stable.latest.default.override {
                  extensions = [ "rust-src" "rust-analyzer" ];
                })
                cargo-watch
                xcb-util-cursor
              ];

              buildInputs = with pkgs;[
                pkg-config
                systemd
              ] ++ commonArgs.buildInputs;

              nativeBuildInputs = with pkgs; [
                virt-viewer
              ];

              shellHook = ''
                source .nixos-vm/vm.sh
                alias cargo="RUST_LOG=debug cargo"
                alias cargo2="RUST_LOG=debug DISPLAY=:2 cargo"
              '';

              LD_LIBRARY_PATH = "${pkgs.lib.makeLibraryPath libPath}";
            };
        };

      flake = {
        formatter.x86_64-linux = nixpkgs.legacyPackages.x86_64-linux.nixpkgs-fmt;
        overlays.default = final: prev: {
          buddaraysh = self.packages.${final.system}.buddaraysh;
        };

        # nixos development vm
        nixosConfigurations.buddaraysh = nixpkgs.lib.nixosSystem
          {
            system = "x86_64-linux";
            modules = [
              {
                nixpkgs.overlays = [
                  self.overlays.default
                ];
              }
              "${nixpkgs}/nixos/modules/virtualisation/qemu-vm.nix"
              ./.nixos-vm/configuration.nix
            ];
          };
      };
    };
}

