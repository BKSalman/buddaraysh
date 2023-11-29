{ config, lib, pkgs, ... }:

with lib;

let
  cfg = config.programs.buddaraysh;
in
{
  ###### interface
  options = {
    programs.buddaraysh.enable = mkEnableOption (lib.mdDoc "buddaraysh");
  };

  ###### implementation
  config = mkIf cfg.enable {
    environment.systemPackages = [ pkgs.buddaraysh ];
  };
}
