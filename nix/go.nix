{pkgs}: {
  packages = with pkgs; [
    go
    gopls
    gofumpt
    golangci-lint
    # Proto tooling
    protobuf
    buf
    protoc-gen-go
    protoc-gen-go-grpc
  ];
}
