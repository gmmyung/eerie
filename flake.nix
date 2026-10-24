{
  description = "Development shell for eerie";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
  };

  outputs =
    { nixpkgs, ... }:
    let
      systems = [
        "aarch64-darwin"
        "x86_64-darwin"
        "x86_64-linux"
        "aarch64-linux"
      ];

      forAllSystems =
        f:
        nixpkgs.lib.genAttrs systems (
          system:
          f {
            pkgs = import nixpkgs { inherit system; };
          }
        );
    in
    {
      devShells = forAllSystems (
        { pkgs }:
        let
          lib = pkgs.lib;
          llvm = pkgs.llvmPackages;
          meliorLlvm = pkgs.llvmPackages_22;
          meliorToolchain = pkgs.symlinkJoin {
            name = "eerie-melior-llvm";
            paths = [
              meliorLlvm.llvm.dev
              meliorLlvm.llvm.lib
              meliorLlvm.mlir.dev
              meliorLlvm.mlir
            ];
          };
          meliorConfig = pkgs.writeShellScriptBin "llvm-config" ''
            case " $* " in
              *" --includedir "*) echo "${meliorToolchain}/include" ;;
              *" --libdir "*) echo "${meliorToolchain}/lib" ;;
              *) exec "${meliorLlvm.llvm.dev}/bin/llvm-config" "$@" ;;
            esac
          '';
          python = pkgs.python3.withPackages (ps: [
            ps.pip
            ps.setuptools
            ps.virtualenv
            ps.wheel
          ]);

          darwinPackages = lib.optionals pkgs.stdenv.isDarwin [
            pkgs.libiconv
          ];
        in
        rec {
          default = pkgs.mkShell {
            packages =
              [
                pkgs.cargo
                pkgs.rustc
                pkgs.rustfmt
                pkgs.clippy
                pkgs.cmake
                pkgs.ninja
                pkgs.pkg-config
                pkgs.git
                pkgs.cacert
                pkgs.openssl
                llvm.clang
                llvm.libclang
                python
              ]
              ++ darwinPackages;

            LIBCLANG_PATH = "${llvm.libclang.lib}/lib";
            CC = "${llvm.clang}/bin/clang";
            CXX = "${llvm.clang}/bin/clang++";
            CMAKE_GENERATOR = "Ninja";
            SSL_CERT_FILE = "${pkgs.cacert}/etc/ssl/certs/ca-bundle.crt";

            shellHook = ''
              if [ -z "''${EERIE_PYTHON:-}" ]; then
                : "''${XDG_CACHE_HOME:=$HOME/.cache}"
                export EERIE_IREE_VENV="''${EERIE_IREE_VENV:-$XDG_CACHE_HOME/eerie/iree-compiler-3.13.0-venv}"
                export EERIE_IREE_COMPILER_PACKAGE="''${EERIE_IREE_COMPILER_PACKAGE:-iree-base-compiler==3.13.0}"
                export EERIE_IREE_COMPILER_DISTRIBUTION="''${EERIE_IREE_COMPILER_DISTRIBUTION:-iree-base-compiler}"
                export EERIE_IREE_COMPILER_VERSION="''${EERIE_IREE_COMPILER_VERSION:-3.13.0}"

                if [ ! -x "$EERIE_IREE_VENV/bin/python" ]; then
                  mkdir -p "$(dirname "$EERIE_IREE_VENV")"
                  ${python}/bin/python -m virtualenv "$EERIE_IREE_VENV"
                fi

                if ! "$EERIE_IREE_VENV/bin/python" -c 'import importlib.metadata as m, os; import iree.compiler; assert m.version(os.environ["EERIE_IREE_COMPILER_DISTRIBUTION"]) == os.environ["EERIE_IREE_COMPILER_VERSION"]' >/dev/null 2>&1; then
                  "$EERIE_IREE_VENV/bin/python" -m pip install --upgrade pip || exit 1
                  "$EERIE_IREE_VENV/bin/pip" install --upgrade --force-reinstall "$EERIE_IREE_COMPILER_PACKAGE" || exit 1
                fi

                export PATH="$EERIE_IREE_VENV/bin:$PATH"
              fi
            '';
          };
          # Melior owns a separate LLVM/MLIR build. Keep its large dependencies
          # out of the ordinary shell and use this for coexistence regression tests.
          melior = default.overrideAttrs (old: {
            nativeBuildInputs = old.nativeBuildInputs ++ [
              meliorLlvm.llvm.dev
              meliorLlvm.llvm.lib
              meliorLlvm.mlir
              pkgs.zlib
              pkgs.libxml2
            ];
            MLIR_SYS_220_PREFIX = "${meliorConfig}";
            TABLEGEN_220_PREFIX = "${meliorConfig}";
            "CARGO_TARGET_${lib.toUpper (builtins.replaceStrings [ "-" ] [ "_" ] pkgs.stdenv.hostPlatform.rust.rustcTarget)}_LINKER" = "${meliorLlvm.clang}/bin/clang";
          });
        }
      );
    };
}
