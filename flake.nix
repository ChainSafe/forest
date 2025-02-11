{
  description = "Forest - A Rust implementation of Filecoin";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-24.11";
    crane.url = "github:ipetkov/crane";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = {
    self,
    nixpkgs,
    crane,
    flake-utils,
    rust-overlay,
    ...
  }:
    flake-utils.lib.eachDefaultSystem (system: let
      pkgs = import nixpkgs {
        inherit system;
        overlays = [(import rust-overlay)];
      };

      rustToolchain = pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;
      craneLib = (crane.mkLib pkgs).overrideToolchain rustToolchain;

      src = pkgs.lib.cleanSourceWith {
        src = ./.;
        filter = path: type:
          (craneLib.filterCargoSources path type)
          || (type == "regular" && pkgs.lib.hasSuffix ".stpl" path)
          || (type == "regular" && pkgs.lib.hasSuffix ".json" path)
          || (type == "regular" && pkgs.lib.hasSuffix ".car" path)
          || (type == "regular" && pkgs.lib.hasSuffix ".txt" path)
          || (pkgs.lib.hasInfix "/build/" path)
          || (pkgs.lib.hasInfix "/f3-sidecar/" path)
          || (pkgs.lib.hasInfix "/test-snapshots/" path);
      };

      # Common arguments can be set here to avoid repeating them later
      commonArgs = {
        inherit src;

        buildInputs = with pkgs; [
          # Add runtime dependencies here
        ];

        nativeBuildInputs = with pkgs; [
          # Add build-time dependencies here
          go # For rust2go compilation
        ];

        doCheck = false;
      };

      # Build *just* the cargo dependencies, so we can reuse
      # all of that work (e.g. via cachix) when running in CI
      cargoArtifacts = craneLib.buildDepsOnly commonArgs;

      # Build the actual crate itself, reusing the dependency
      # artifacts from above.
      forest = craneLib.buildPackage (commonArgs
        // {
          inherit cargoArtifacts;
          # Set HOME for Go compilation
          preConfigure = ''
            export HOME=$(mktemp -d)
          '';
          # Environment variables needed for the build
          # FOREST_F3_SIDECAR_FFI_BUILD_OPT_OUT = "1";
        });
    in {
      checks = {
        # Build the crate as part of `nix flake check` for convenience
        inherit forest;
      };

      packages.default = forest;

      apps = let
        binaries = ["forest" "forest-cli" "forest-tool" "forest-wallet"];
        mkBinApp = name:
          flake-utils.lib.mkApp {
            drv = forest;
            inherit name;
          };
      in
        {
          default = mkBinApp "forest";
        }
        // builtins.listToAttrs (map (name: {
            inherit name;
            value = mkBinApp name;
          })
          binaries);

      devShells.default = pkgs.mkShell {
        inputsFrom = builtins.attrValues self.checks.${system};

        # Additional dev-shell environment variables can be set directly
        shellHook = ''
          echo "Forest development shell"
        '';
      };
    });
}
