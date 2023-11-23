{ config, lib, pkgs, ... }:

with lib;

let
  cfg = config.services.xserver.windowManager.buddaraysh;
in
{
  ###### interface
  options = {
    services.xserver.windowManager.buddaraysh.enable = mkEnableOption (lib.mdDoc "buddaraysh");
  };

  ###### implementation
  config = mkIf cfg.enable {
    services.xserver.windowManager.session = singleton {
      name = "buddaraysh";
      start = ''
        ${pkgs.buddaraysh}/bin/buddaraysh &
        waitPID=$!
      '';
    };
    environment.systemPackages = [ pkgs.buddaraysh ];
  };
}

