{ pkgs ? import <nixpkgs> { } }:
let
  fenix = import
    (fetchTarball "https://github.com/nix-community/fenix/archive/main.tar.gz")
    { };
in pkgs.mkShell {
  buildInputs = with pkgs; [
  wasm-bindgen-cli
    wasm-pack
    pkg-config
    openssl
    nodePackages.eslint
    nodePackages.prettier
    nodePackages.typescript-language-server
    nodejs
      nodePackages.typescript
    # flow

    (with fenix;
      # combine (with beta; [
     combine (with latest; [
        cargo
        clippy-preview
        latest.rust-src
        rust-analyzer
        rust-std
        targets.wasm32-unknown-unknown.latest.rust-std
        rustc
        rustfmt-preview
      ]))
  ];
}
