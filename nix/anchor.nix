{
  lib,
  stdenv,
  rustPlatform,
  fetchFromGitHub,
  pkg-config,
  openssl,
  perl,
  hidapi,
  udev,
}:
rustPlatform.buildRustPackage rec {
  pname = "anchor";
  version = "1.0.0";

  src = fetchFromGitHub {
    owner = "solana-foundation";
    repo = "anchor";
    tag = "v${version}";
    hash = "sha256-+VCY7By3Xh71opOSY+/xXd9LmsmYq0SstwKOfrI9KKU=";
  };

  cargoHash = "sha256-GH/R7S8jQAWGTz8Ig/u/yb9o6FPtmkAaOzgl0uiB0dk=";

  nativeBuildInputs = [perl pkg-config];

  buildInputs = [openssl] ++ lib.optionals stdenv.isLinux [hidapi udev];

  checkFlags = [
    "--skip=tests::test_check_and_get_full_commit_when_full_commit"
    "--skip=tests::test_check_and_get_full_commit_when_partial_commit"
    "--skip=tests::test_get_anchor_version_from_commit"
  ];

  meta = with lib; {
    description = "Solana Sealevel Framework";
    homepage = "https://github.com/solana-foundation/anchor";
    changelog = "https://github.com/solana-foundation/anchor/blob/${src.rev}/CHANGELOG.md";
    license = licenses.asl20;
    mainProgram = "anchor";
  };
}
