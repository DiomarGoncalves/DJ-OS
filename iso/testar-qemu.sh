#!/bin/bash
# Liga a ISO numa máquina virtual. Uso:
#   iso/testar-qemu.sh            → janela, BIOS antiga
#   iso/testar-qemu.sh uefi       → janela, UEFI
#   SEM_TELA=1 iso/testar-qemu.sh → sem janela (testes automáticos: QMP e terminal serial)
#   sudo REV1=1 iso/testar-qemu.sh → com a DDJ-REV1 ligada dentro da VM
#   PENDRIVE=img SEM_SOM=1 …       → pendrive USB virtual / sem placa de som
#   DISCO=img INSTALAR=vda …        → instala sozinho no disco virtual; SO_DISCO=1 liga por ele
set -euo pipefail
# Com sudo, usa a pasta do usuário de verdade (não a do root).
H=$(getent passwd "${SUDO_USER:-$USER}" | cut -d: -f6)
ISO=${ISO:-$H/DJOS/iso/dj-os.iso}
T=$H/DJOS/iso/trabalho; mkdir -p "$T"
ARGS=(-enable-kvm -m 3072 -smp 2 -device qemu-xhci -device usb-tablet)
# DISCO=<imagem>: disco interno virtual (para testar a instalação). SO_DISCO=1: liga pelo disco,
# sem a ISO. INSTALAR=<disco>: liga a ISO direto no instalador automático (ex.: INSTALAR=vda).
[ -z "${DISCO:-}" ] || ARGS+=(-drive "file=$DISCO,format=raw,if=virtio")
if [ -n "${SO_DISCO:-}" ]; then
    ARGS+=(-boot c)
elif [ -n "${INSTALAR:-}" ]; then
    L=$T/iso/live
    ARGS+=(-cdrom "$ISO" -kernel "$L/vmlinuz" -initrd "$L/initrd.img"
           -append "boot=live components quiet loglevel=3 djos.instalar=auto:$INSTALAR")
else
    ARGS+=(-cdrom "$ISO" -boot d)
fi
# SEM_SOM=1: máquina sem placa de som (como um VirtualBox com o áudio desligado).
[ -n "${SEM_SOM:-}" ] || ARGS+=(-audiodev none,id=a -device intel-hda -device hda-output,audiodev=a)
# PENDRIVE=<imagem>: liga uma imagem de disco como pendrive USB
# (vários separados por vírgula)
n=0
for img in ${PENDRIVE:+${PENDRIVE//,/ }}; do
    n=$((n + 1))
    ARGS+=(-drive "if=none,id=pd$n,format=raw,file=$img" -device "usb-storage,drive=pd$n")
done
# VGA=vmware imita a placa VMSVGA do VirtualBox; VGA=std e VGA=cirrus, placas simples.
case "${VGA:-virtio}" in
    virtio) ARGS+=(-device virtio-vga) ;;
    *) ARGS+=(-vga "$VGA") ;;
esac
if [ "${1:-}" = uefi ]; then
    cp /usr/share/OVMF/OVMF_VARS_4M.fd "$T/ovmf-vars.fd" 2>/dev/null || cp /usr/share/OVMF/OVMF_VARS.fd "$T/ovmf-vars.fd"
    ARGS+=(-drive if=pflash,format=raw,readonly=on,file=/usr/share/OVMF/OVMF_CODE_4M.fd
           -drive if=pflash,format=raw,file="$T/ovmf-vars.fd")
fi
# REV1=1: passa a DDJ-REV1 (USB) para dentro da VM, para testar controladora, master e fone.
# O Linux do PC deixa de usar a REV1 enquanto a VM estiver ligada.
if [ -n "${REV1:-}" ]; then
    ID=$(lsusb | grep -i 'DDJ-REV1\|Pioneer' | head -1 | grep -o 'ID [0-9a-f]*:[0-9a-f]*' | cut -d' ' -f2)
    [ -n "$ID" ] || { echo "REV1 não encontrada no USB"; exit 1; }
    BUS=$(lsusb | grep "$ID" | head -1 | awk '{print $2"/"$4}' | tr -d :)
    if [ ! -w "/dev/bus/usb/$BUS" ]; then
        echo "Sem permissão no USB da REV1. Rode assim: sudo REV1=1 $0 $*"; exit 1
    fi
    ARGS+=(-device "usb-host,vendorid=0x${ID%%:*},productid=0x${ID##*:}")
fi
if [ -n "${SEM_TELA:-}" ]; then
    ARGS+=(-display none -qmp unix:$T/qmp.sock,server,nowait -serial unix:$T/serial.sock,server,nowait)
fi
exec nice -n 10 qemu-system-x86_64 "${ARGS[@]}"
