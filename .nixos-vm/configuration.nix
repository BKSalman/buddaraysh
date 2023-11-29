{ config, pkgs, ... }:
{
  imports = [
    ./buddaraysh.nix
  ];

  system.stateVersion = "23.05";
  boot.initrd.availableKernelModules = [ "virtio_net" "virtio_pci" "virtio_mmio" "virtio_blk" "virtio_scsi" "9p" "9pnet_virtio" ];
  boot.initrd.kernelModules = [ "virtio_balloon" "virtio_console" "virtio_rng" ];

  virtualisation.diskImage = "./.nixos-vm/${config.system.name}.qcow2";

  users.users.buddaraysh = {
    isNormalUser = true;
    extraGroups = [ "wheel" ];
    initialPassword = "123";
    home = "/home/buddaraysh";
  };

  xdg.portal = {
    enable = true;
    config.common.default = "gtk";
    extraPortals = [
      pkgs.xdg-desktop-portal-gtk
    ];
  };

  environment.sessionVariables = {
    __GL_GSYNC_ALLOWED = "0";
    __GL_VRR_ALLOWED = "0";
    WLR_DRM_NO_ATOMIC = "1";
    _JAVA_AWT_WM_NONREPARENTING = "1";
    QT_QPA_PLATFORM = "wayland;xcb";
    # TODO: put back later
    # QT_WAYLAND_DISABLE_WINDOWDECORATION = "1";
    GDK_BACKEND = "wayland,x11";
    WLR_NO_HARDWARE_CURSORS = "1";
    MOZ_ENABLE_WAYLAND = "1";
    WLR_BACKEND = "vulkan";
    WLR_RENDERER = "vulkan";
    XCURSOR_SIZE = "24";
    NIXOS_OZONE_WL = "1";
    GTK_USE_PORTAL = "1";
    PATH = [
      "$HOME/.local/bin/:$PATH"
    ];
  };

  networking = {
    hostName = "buddaraysh";
    networkmanager.enable = true;
  };

  environment.systemPackages = with pkgs; [
    firefox
    btop
    magic-wormhole
    kitty
    hyprpicker
    slurp
    grim
    waybar
    xorg.xcalc
    jq
    vim
  ];

  programs.git.enable = true;
  services = {
    openssh.enable = true;
    spice-vdagentd.enable = true;
    qemuGuest.enable = true;

    xserver = {
      videoDrivers = [ "qxl" ];
      enable = true;

      # desktopManager.xterm.enable = true;
      displayManager.autoLogin.enable = true;
      displayManager.autoLogin.user = "buddaraysh";
    };
  };

  programs.buddaraysh.enable = true;

  services.greetd = {
    enable = true;
    settings = rec {
      initial_session = {
        command = "buddaraysh";
        user = "buddaraysh";
      };
      default_session = initial_session;
    };
  };

  environment.etc."greetd/environments".text = ''
    buddaraysh
  '';
}
