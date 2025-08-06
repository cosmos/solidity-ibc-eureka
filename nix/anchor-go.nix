{ pkgs }:

pkgs.buildGoModule rec {
  pname = "anchor-go";
  version = "1.0.0";

  src = pkgs.fetchFromGitHub {
    owner = "gagliardetto";
    repo = "anchor-go";
    rev = "v${version}";
    sha256 = "sha256-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=";
  };

  vendorSha256 = "sha256-BBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB";

  # optionally you can enable tests if needed
  doCheck = false;

  meta = with pkgs.lib; {
    description = "Golang Anchor client";
    homepage = "https://github.com/gagliardetto/anchor-go";
    license = licenses.mit;
    maintainers = with maintainers; [ ];
  };
}
