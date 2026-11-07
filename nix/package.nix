{
  lib,
  rustPlatform,
}:
let
  manifest = builtins.fromTOML (builtins.readFile ../Cargo.toml);
  pname = "glimpse-monitor";
  appletId = "lt.nyv.glimpse-monitor";
in
rustPlatform.buildRustPackage {
  pname = pname;
  version = manifest.package.version;

  src = lib.cleanSourceWith {
    src = ../.;
    filter =
      path: type:
      let
        root = toString ../.;
        relative = lib.removePrefix "${root}/" (toString path);
        top = builtins.head (lib.splitString "/" relative);
      in
      toString path == root
      || builtins.elem top [
        "Cargo.toml"
        "Cargo.lock"
        "src"
        "data"
        "${appletId}.desktop"
      ];
  };

  cargoLock.lockFile = ../Cargo.lock;
  doCheck = false;

  postInstall = ''
    install -Dm644 "${appletId}.desktop" "$out/share/applications/${appletId}.desktop"
    substituteInPlace "$out/share/applications/${appletId}.desktop" \
      --replace-fail "Exec=./${pname}" "Exec=$out/bin/${pname}"

    mkdir -p "$out/share/icons/hicolor/scalable/status"
    for icon in data/icons/*; do
      install -m0644 "$icon" "$out/share/icons/hicolor/scalable/status/${pname}-$(basename "$icon")"
    done
  '';

  meta = {
    description = "System monitor widgets for the Glimpse desktop panel";
    license = lib.licenses.bsd3;
    platforms = [
      "x86_64-linux"
      "aarch64-linux"
    ];
  };
}
