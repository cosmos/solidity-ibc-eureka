{pkgs}: let
  # toolchain > 1.92 breaks on transitive dep alloy-signer-aws-1.5.2
  # queries overflow the depth limit!
  # TODO: remove post alloy bump
  toolchain = pkgs.rust-bin.stable."1.92.0".default.override {
    extensions = ["rust-src" "rust-analyzer" "clippy" "rustfmt"];
  };
in {
  packages = with pkgs;
    [
      toolchain
      pkg-config
      openssl
      openssl.dev
      sccache
      cargo-nextest
    ]
    ++ lib.optionals stdenv.isDarwin [
      apple-sdk_15
    ];

  env = {
    RUST_SRC_PATH = "${toolchain}/lib/rustlib/src/rust/library";
  };

  NIX_LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath [pkgs.stdenv.cc.cc];

  shellHook = ''
    # WORKAROUND: Fix Darwin SDK conflicts
    # nixpkgs unstable has mismatched Apple SDK versions causing linker errors.
    if [[ "$OSTYPE" == "darwin"* ]]; then
      unset DEVELOPER_DIR_FOR_TARGET
      unset NIX_APPLE_SDK_VERSION_FOR_TARGET
      unset SDKROOT_FOR_TARGET
    fi
  '';

  inherit toolchain;
}
