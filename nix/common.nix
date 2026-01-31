{pkgs}: {
  packages = with pkgs; [
    bun
    just
    jq
    parallel
    quicktype
  ];
}
