{
  mkShell,
  rustc,
  cargo,
  clippy,
  rustfmt,
  rust-analyzer,
  rustPlatform,
  just,
}:
mkShell {
  packages = [
    just
    cargo
    rustc
    rustfmt
    clippy
    rust-analyzer
  ];

  env.RUST_SRC_PATH = "${rustPlatform.rustLibSrc}";
}
