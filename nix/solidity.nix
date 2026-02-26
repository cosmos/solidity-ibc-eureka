{
  pkgs,
  inputs,
  system,
}: {
  packages = with pkgs; [
    go-ethereum
    solc_0_8_28
    (inputs.solc.mkDefault pkgs solc_0_8_28)
    slither-analyzer
    inputs.natlint.packages.${system}.default
  ];
}
