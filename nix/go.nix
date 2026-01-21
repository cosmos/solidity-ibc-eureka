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

  shellHook = ''
    export GOPATH=$HOME/go
    export PATH=$GOPATH/bin:$PATH
  '';
}
