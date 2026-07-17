{
  lib,
  stdenv,
  rustPlatform,
  curl,
  jq,
  fetchFromGitHub,
  pkg-config,
  openssl,
  perl,
  hidapi,
  udev,
}:
rustPlatform.buildRustPackage rec {
  pname = "anchor";
  version = "1.1.2";

  src = fetchFromGitHub {
    owner = "otter-sec";
    repo = "anchor";
    tag = "v${version}";
    hash = "sha256-UV5aquH0YqCpex9LDrDdmZmdebhUXKqqsM6X/d2vJIs=";
  };

  cargoHash = "sha256-oEgWfklxjP8+TxrhDKJgcTsanpqJpEiHXJyir8neYj8=";

  nativeBuildInputs = [perl pkg-config curl jq];

  buildInputs = [openssl] ++ lib.optionals stdenv.isLinux [hidapi udev];

  checkFlags = [
    "--skip=tests::test_check_and_get_full_commit_when_full_commit"
    "--skip=tests::test_check_and_get_full_commit_when_partial_commit"
    "--skip=tests::test_get_anchor_version_from_commit"
  ];

  meta = with lib; {
    description = "Solana Sealevel Framework";
    homepage = "https://github.com/otter-sec/anchor";
    changelog = "https://github.com/otter-sec/anchor/blob/${src.rev}/CHANGELOG.md";
    license = licenses.asl20;
    mainProgram = "anchor";
  };
}
