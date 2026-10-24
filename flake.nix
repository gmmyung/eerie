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
          compilerVersion = "3.13.0";
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
            ps.virtualenv
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
                pkgs.git
                pkgs.cacert
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
                compiler_venv="''${XDG_CACHE_HOME:-$HOME/.cache}/eerie/iree-compiler-${compilerVersion}-venv"

                if [ ! -x "$compiler_venv/bin/python" ]; then
                  mkdir -p "$(dirname "$compiler_venv")"
                  ${python}/bin/python -m virtualenv "$compiler_venv"
                fi

                if ! "$compiler_venv/bin/python" -c 'import importlib.metadata as m; assert m.version("iree-base-compiler") == "${compilerVersion}"' >/dev/null 2>&1; then
                  "$compiler_venv/bin/python" -m pip install --upgrade pip || exit 1
                  "$compiler_venv/bin/pip" install "iree-base-compiler==${compilerVersion}" || exit 1
                fi

                export PATH="$compiler_venv/bin:$PATH"
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
