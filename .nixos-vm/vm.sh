#!/usr/bin/env bash

SCRIPTPATH=$( cd -- "$( dirname -- "${BASH_SOURCE[0]}" )" &> /dev/null && pwd )

QEMU_OPTS="
  -vga qxl
  -spice unix=on,addr=/tmp/vm_spice.socket,disable-ticketing=on
  -device virtio-serial-pci
  -chardev spicevmc,id=spicechannel0,name=vdagent
  -device virtserialport,chardev=spicechannel0,name=com.redhat.spice.0
"

bu-vm () {
  if [ "$1" = "clean" ]; then 

    rm -rf "${SCRIPTPATH}/result"
    rm "${SCRIPTPATH}/buddaraysh.qcow2"

  else

  echo building a buddaraysh virtual machine...

  nix build .#nixosConfigurations.buddaraysh.config.system.build.vm -o "${SCRIPTPATH}/result" -L

  QEMU_OPTS=$QEMU_OPTS $SCRIPTPATH/result/bin/run-buddaraysh-vm & 
  PID_QEMU="$!"

  sleep 1
  remote-viewer spice+unix:///tmp/vm_spice.socket
  kill $PID_QEMU

  fi
}

