{
  description = "System monitor widgets for the Glimpse desktop panel";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-26.05";
    glimpse.url = "github:alex-oleshkevich/glimpse";
  };

  outputs =
    {
      self,
      nixpkgs,
      glimpse,
    }:
    let
      systems = [
        "x86_64-linux"
        "aarch64-linux"
      ];

      forAllSystems = f: nixpkgs.lib.genAttrs systems (system: f nixpkgs.legacyPackages.${system});
    in
    {
      packages = forAllSystems (pkgs: rec {
        glimpse-monitor = pkgs.callPackage ./nix/package.nix { };
        default = glimpse-monitor;
      });

      devShells = forAllSystems (pkgs: {
        default = pkgs.callPackage ./nix/devshell.nix { };
      });
    };
}
