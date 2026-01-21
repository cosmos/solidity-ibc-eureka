{ pkgs }:

let
  src = pkgs.lib.cleanSourceWith {
    src = ../.;
    filter = path: type:
      let
        baseName = builtins.baseNameOf path;
      in
      baseName == "package.json"
      || baseName == "bun.lock"
      || baseName == "bunfig.toml";
  };

in
pkgs.stdenv.mkDerivation {
  pname = "node-modules";
  version = "1.0.0";

  inherit src;

  nativeBuildInputs = [
    pkgs.bun
    pkgs.jq
  ];

  buildPhase = ''
    export HOME=$TMPDIR
    export BUN_INSTALL_CACHE_DIR=$TMPDIR/bun-cache
    bun install --frozen-lockfile --no-progress
  '';

  installPhase = ''
    mkdir -p $out
    cp -R node_modules $out/
  '';

  outputHashMode = "recursive";
  outputHashAlgo = "sha256";
  outputHash = "sha256-dXMK+OyDvBmG3dFngt6nFaIZ02q2bqOUrNZqbOLmrNk=";
}
