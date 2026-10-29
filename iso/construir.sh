#!/bin/bash
# Monta a ISO do DJ OS (Ubuntu 24.04 mínimo + o sistema), sem precisar de sudo.
#
#   iso/construir.sh   → ~/DJOS/iso/dj-os-<versão>.iso  e  dj-os-firmware-<versão>.djos
#
# Versão = a do Cargo.toml (maior.menor.correção) + data e hora da montagem, ex.:
# 0.1.0-20261029.1245. Para lançar uma versão nova: suba a versão no Cargo.toml, escreva o
# CHANGELOG.md e monte (veja docs/versoes.md). O atalho dj-os.iso aponta para a mais recente.
#
# Etapas: compila o sistema, cria a raiz do Linux (mmdebstrap, modo "unshare"), comprime em
# squashfs e gera a ISO híbrida (BIOS e UEFI) com o GRUB. A ISO funciona no Ventoy.
set -euo pipefail

PROJ=$(cd "$(dirname "$0")/.." && pwd)
SAIDA=${SAIDA:-$HOME/DJOS/iso}
TRAB=$SAIDA/trabalho
VERSAO="$(sed -n 's/^version = "\(.*\)"/\1/p' "$PROJ/Cargo.toml" | head -1)-$(date +%Y%m%d.%H%M)"
export PATH="$HOME/DJOS/ferramentas/mtools/usr/bin:$PATH"

mkdir -p "$TRAB" "$SAIDA"
echo "$VERSAO" > "$TRAB/versao"

# Chave do firmware: a secreta fica só neste PC; a pública vai dentro do sistema.
CHAVE_DIR=$HOME/DJOS/chave
if [ ! -f "$CHAVE_DIR/firmware.key" ]; then
    mkdir -p "$CHAVE_DIR" && chmod 700 "$CHAVE_DIR"
    openssl genpkey -algorithm ed25519 -out "$CHAVE_DIR/firmware.key"
    chmod 600 "$CHAVE_DIR/firmware.key"
    echo "Chave nova do firmware criada em $CHAVE_DIR (guarde uma cópia em lugar seguro)"
fi
openssl pkey -in "$CHAVE_DIR/firmware.key" -pubout -out "$CHAVE_DIR/firmware.pub"
echo "== 1/4 compilando o DJ OS (versão $VERSAO)"
# Neste PC compila sem internet (as dependências já estão baixadas); no GitHub (DJOS_CI=1) baixa.
OFFLINE=--offline
[ -n "${DJOS_CI:-}" ] && OFFLINE=
(cd "$PROJ" && nice -n 19 "$HOME/.cargo/bin/cargo" build --release $OFFLINE -j 4)

PACOTES=(
    # Base e boot ao vivo
    linux-image-generic initramfs-tools zstd live-boot live-boot-initramfs-tools plymouth fontconfig fonts-dejavu-core
    systemd-sysv udev dbus kmod iproute2 ca-certificates tzdata
    # Som: PipeWire igual ao do PC de desenvolvimento (Ubuntu 24.04)
    pipewire pipewire-bin pipewire-alsa wireplumber libspa-0.2-bluetooth libasound2t64 alsa-utils
    # Tela: compositor de uma janela só e drivers de vídeo (Intel antiga inclusa)
    cage libgl1-mesa-dri libegl1 libgl1 libgles2 libwayland-egl1 libwayland-client0
    libwayland-cursor0 libxkbcommon0 dmz-cursor-theme
    # Bluetooth, Wi-Fi e pendrives
    bluez rfkill network-manager wpasupplicant ntfs-3g exfatprogs dosfstools
    # Manutenção e o instalador (próxima etapa)
    whiptail sudo openssl keyboard-configuration console-setup less nano pciutils usbutils parted gdisk rsync squashfs-tools
    grub-pc-bin grub-efi-amd64-bin grub2-common efibootmgr
)

echo "== 2/4 criando o sistema (baixa ~700 MB na primeira vez; depois usa o cache)"
CACHE=$SAIDA/cache-apt
mkdir -p "$CACHE"
nice -n 19 mmdebstrap --mode=unshare --variant=apt \
    --skip=essential/unlink \
    --setup-hook='mkdir -p "$1"/var/cache/apt/archives/' \
    --setup-hook="sync-in '$CACHE' /var/cache/apt/archives/" \
    --components=main,universe \
    --include="$(IFS=,; echo "${PACOTES[*]}")" \
    --aptopt='APT::Install-Recommends "false"' \
    --dpkgopt='path-exclude=/usr/share/doc/*' \
    --dpkgopt='path-exclude=/usr/share/man/*' \
    --dpkgopt='path-exclude=/usr/share/locale/*' \
    --dpkgopt='path-include=/usr/share/locale/pt_BR/*' \
    --customize-hook="sync-in '$PROJ/iso/raiz' /" \
    --customize-hook="upload '$PROJ/target/release/dj-os' /usr/local/bin/dj-os" \
    --customize-hook="upload '$CHAVE_DIR/firmware.pub' /usr/share/dj-os/firmware.pub" \
    --customize-hook="upload '$PROJ/assets/logo-dj-os.png' /usr/share/plymouth/themes/dj-os/logo.png" \
    --customize-hook='chroot "$1" update-alternatives --install /usr/share/plymouth/themes/default.plymouth default.plymouth /usr/share/plymouth/themes/dj-os/dj-os.plymouth 200' \
    --customize-hook='chroot "$1" update-alternatives --set default.plymouth /usr/share/plymouth/themes/dj-os/dj-os.plymouth' \
    --customize-hook='chroot "$1" systemctl mask plymouth-poweroff.service plymouth-reboot.service plymouth-halt.service plymouth-kexec.service' \
    --customize-hook='chroot "$1" chmod 755 /usr/local/bin/dj-os' \
    --customize-hook='chroot "$1" sh -c "chown -R root:root /usr/local /etc/sudoers.d /etc/systemd /etc/plymouth /usr/share/dj-os /usr/share/plymouth/themes/dj-os && chmod 440 /etc/sudoers.d/dj-os && chmod 755 /usr/local/sbin/* /usr/local/bin/*"' \
    --customize-hook='chroot "$1" useradd -m -u 1000 -s /bin/bash -G audio,video,input,render,bluetooth,plugdev,adm,systemd-journal dj' \
    --customize-hook='chroot "$1" passwd -d dj' \
    --customize-hook='chroot "$1" ln -sf /usr/share/zoneinfo/America/Sao_Paulo /etc/localtime' \
    --customize-hook='echo "LANG=C.UTF-8" > "$1/etc/default/locale"' \
    --customize-hook="echo '$VERSAO' > \"\$1/etc/dj-os-versao\"" \
    --customize-hook='chroot "$1" systemctl enable NetworkManager bluetooth serial-getty@ttyS0 dj-os-dados dj-os-instalador' \
    --customize-hook='chroot "$1" systemctl mask systemd-networkd-wait-online.service NetworkManager-wait-online.service' \
    --customize-hook='chroot "$1" update-initramfs -u -k all' \
    --customize-hook="sync-out /var/cache/apt/archives '$CACHE'" \
    --customize-hook='rm -rf "$1/var/cache/apt/archives/"*.deb "$1/var/lib/apt/lists/"*' \
    noble "$TRAB/raiz.tar" \
    "deb http://archive.ubuntu.com/ubuntu noble main universe" \
    "deb http://archive.ubuntu.com/ubuntu noble-updates main universe" \
    "deb http://security.ubuntu.com/ubuntu noble-security main universe"

echo "== 3/4 comprimindo (squashfs)"
rm -rf "$TRAB/iso" && mkdir -p "$TRAB/iso/live" "$TRAB/iso/boot/grub"
nice -n 19 sqfstar -comp xz -processors 4 -quiet "$TRAB/iso/live/filesystem.squashfs" < "$TRAB/raiz.tar"
# Kernel e initrd saem do próprio pacote do sistema.
rm -rf "$TRAB/kernel" && mkdir -p "$TRAB/kernel"
tar -xf "$TRAB/raiz.tar" -C "$TRAB/kernel" --wildcards './boot/vmlinuz-*' './boot/initrd.img-*'
cp "$(ls "$TRAB"/kernel/boot/vmlinuz-* | sort -V | tail -1)" "$TRAB/iso/live/vmlinuz"
cp "$(ls "$TRAB"/kernel/boot/initrd.img-* | sort -V | tail -1)" "$TRAB/iso/live/initrd.img"
# Impressões digitais: o instalador confere se a ISO chegou inteira no pendrive.
(cd "$TRAB/iso/live" && sha256sum vmlinuz initrd.img filesystem.squashfs > SHA256SUMS)
sed "s/@VERSAO@/$VERSAO/" "$PROJ/iso/grub.cfg" > "$TRAB/iso/boot/grub/grub.cfg"

echo "== 4/4 gerando a ISO (BIOS e UEFI)"
ISO="$SAIDA/dj-os-$VERSAO.iso"
grub-mkrescue -o "$ISO" "$TRAB/iso" -- -volid DJ_OS 2>&1 | grep -v "^xorriso : UPDATE" || true
ln -sfn "$(basename "$ISO")" "$SAIDA/dj-os.iso"
ls -lh "$ISO"
echo "== firmware (atualização pelo pendrive)"
"$PROJ/iso/firmware.sh"
echo "Pronto: $ISO (versão $VERSAO)"
