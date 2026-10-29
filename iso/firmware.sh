#!/bin/bash
# Empacota e assina o firmware (atualização pelo pendrive) a partir da última ISO montada.
#
#   iso/firmware.sh   → ~/DJOS/iso/dj-os-firmware-<versão>.djos
#
# A chave secreta fica em ~/DJOS/chave/firmware.key (só neste PC; guarde uma cópia!).
# Sem ela não dá para gerar firmware que o aparelho aceite.
set -euo pipefail
SAIDA=${SAIDA:-$HOME/DJOS/iso}
LIVE=$SAIDA/trabalho/iso/live
CHAVE=$HOME/DJOS/chave/firmware.key
[ -f "$CHAVE" ] || { echo "Sem a chave ($CHAVE). Rode iso/construir.sh uma vez."; exit 1; }
VERSAO=$(cat "$SAIDA/trabalho/versao")
T=$(mktemp -d)
trap 'rm -rf "$T"' EXIT
{
    echo "versao=$VERSAO"
    for f in vmlinuz initrd.img filesystem.squashfs; do
        echo "$f $(sha256sum "$LIVE/$f" | cut -d' ' -f1)"
    done
} > "$T/manifesto"
openssl pkeyutl -sign -inkey "$CHAVE" -rawin -in "$T/manifesto" -out "$T/manifesto.sig"
OUT="$SAIDA/dj-os-firmware-$VERSAO.djos"
# Manifesto primeiro: o aparelho confere a assinatura antes de ler o resto.
tar -cf "$OUT" -C "$T" manifesto manifesto.sig -C "$LIVE" vmlinuz initrd.img filesystem.squashfs
ls -lh "$OUT"
