{
  description =
    "Backup a directory on a linux filesystem to Google Cloud Storage";

  inputs = {
    nixpkgs = { url = "github:nixos/nixpkgs/nixos-23.11"; };

    flakebox = {
      url = "github:rustshop/flakebox";
      inputs.nixpkgs.follows = "nixpkgs";
    };

    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, flakebox, flake-utils }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs { inherit system; };
        lib = pkgs.lib;
        flakeboxLib = flakebox.lib.${system} { };
        rustSrc = flakeboxLib.filterSubPaths {
          root = builtins.path {
            name = "backuplit";
            path = ./.;
          };
          paths = [ "Cargo.toml" "Cargo.lock" ".cargo" "src" ];
        };

        toolchainArgs = {
          # extraRustFlags = "--cfg tokio_unstable";
        } // lib.optionalAttrs pkgs.stdenv.isDarwin {
          # on Darwin newest stdenv doesn't seem to work
          # linking rocksdb
          stdenv = pkgs.clang11Stdenv;
        };

        # all standard toolchains provided by flakebox
        toolchainsStd = flakeboxLib.mkStdFenixToolchains toolchainArgs;

        toolchainsNative = (pkgs.lib.getAttrs [ "default" ] toolchainsStd);

        toolchainNative =
          flakeboxLib.mkFenixMultiToolchain { toolchains = toolchainsNative; };

        commonArgs = {
          buildInputs = [ pkgs.openssl ] ++ lib.optionals pkgs.stdenv.isDarwin
            [ pkgs.darwin.apple_sdk.frameworks.SystemConfiguration ];
          nativeBuildInputs = [ pkgs.pkg-config ];
        };
        outputs = (flakeboxLib.craneMultiBuild { toolchains = toolchainsStd; })
          (craneLib':
            let
              craneLib = (craneLib'.overrideArgs {
                pname = "flexbox-multibuild";
                src = rustSrc;
              }).overrideArgs commonArgs;
            in rec {
              workspaceDeps = craneLib.buildWorkspaceDepsOnly { };
              workspaceBuild =
                craneLib.buildWorkspace { cargoArtifacts = workspaceDeps; };
              backuplit = craneLib.buildPackageGroup {
                pname = "backuplit";
                packages = [ "backuplit" ];
                mainProgram = "backuplit";
              };
            });
      in {
        legacyPackages = outputs;
        packages = { default = outputs.backuplit; };
        devShells = fmLib.devShells // {
          default = fmLib.devShells.default.overrideAttrs (prev: {
            nativeBuildInputs = [ pkgs.mprocs ] ++ prev.nativeBuildInputs;
          });
        };
      });
}
