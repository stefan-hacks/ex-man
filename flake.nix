{
  description = "ex-man - Intelligent manpage example generator. Parses local manpages and generates contextual usage examples.";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, flake-utils }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = nixpkgs.legacyPackages.${system};

        # ex-man derivation - builds from local source
        ex-man = pkgs.rustPlatform.buildRustPackage {
          pname = "ex-man";
          version = "0.1.0";

          src = ./.;

          # Build-time dependencies for rusqlite (bundled SQLite)
          nativeBuildInputs = with pkgs; [
            pkg-config
          ];

          # Runtime dependencies - needed for manpage rendering
          buildInputs = with pkgs; [
            sqlite
          ] ++ pkgs.lib.optionals pkgs.stdenv.hostPlatform.isDarwin [
            pkgs.libiconv
          ];

          # Skip tests in build to avoid needing manpages in sandbox
          doCheck = false;

          # Cargo.lock is not in repo by default
          cargoLock = {
            lockFile = ./Cargo.lock;
          };

          meta = with pkgs.lib; {
            description = "Intelligent manpage example generator";
            longDescription = ''
              ex-man parses local manpages, extracts every flag and option,
              and generates contextual usage examples. It uses semantic analysis
              to create realistic arguments and combination examples.
              Works offline with a local SQLite cache.
            '';
            homepage = "https://github.com/stefan-hacks/ex-man";
            license = licenses.mit;
            maintainers = [ ];
            platforms = platforms.unix;
          };
        };
      in
      {
        # The main package - `nix build .#ex-man` or `nix build .`
        packages.ex-man = ex-man;
        packages.default = ex-man;

        # App definition - `nix run .#ex-man -- --help` or `nix run . -- --help`
        apps.ex-man = flake-utils.lib.mkApp {
          drv = ex-man;
        };
        apps.default = flake-utils.lib.mkApp {
          drv = ex-man;
        };

        # Development shell - `nix develop`
        devShells.default = pkgs.mkShell {
          buildInputs = with pkgs; [
            rustc
            cargo
            rustfmt
            clippy
            pkg-config
            sqlite
          ];

          shellHook = ''
            echo "ex-man development shell"
            echo "  rustc: $(rustc --version)"
            echo "  cargo: $(cargo --version)"
            echo ""
            echo "Quick commands:"
            echo "  cargo build --release    # Build optimized binary"
            echo "  cargo test               # Run tests"
            echo "  cargo run -- ls          # Test with ls manpage"
            echo ""
          '';
        };

        # Formatter - `nix fmt`
        formatter = pkgs.nixfmt-tree;
      }
    );
}