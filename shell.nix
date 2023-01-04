{ pkgs ? import <nixpkgs> { } }:
let
  fenix = import
    (fetchTarball "https://github.com/nix-community/fenix/archive/main.tar.gz")
    { };
in pkgs.mkShell {
  buildInputs = with pkgs; [
    wasm-pack
    pkgconfig
    openssl
    # flow

    (with fenix;
      combine (with default; [
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
