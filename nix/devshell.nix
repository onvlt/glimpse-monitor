{
  mkShell,
  rustc,
  cargo,
  clippy,
  rustfmt,
  rust-analyzer,
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
}
