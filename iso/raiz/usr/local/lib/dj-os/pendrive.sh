# Funções para guardar arquivos num pendrive (usadas pelo instalador e pela cópia de registros).
#
#   no_pendrive <arquivo> <pasta> [disco a pular]
#
# Copia o arquivo para <pendrive>/<pasta> em todas as partes de discos USB que aceitarem
# gravação (FAT32, exFAT, NTFS, ext4). No pendrive do Ventoy a parte grande fica presa pela ISO
# aberta; a pequena (VTOYEFI) aceita. Em $salvos fica a lista de onde gravou (texto para a tela).
no_pendrive() {
    local arq=$1 pasta=$2 pular=${3:-} m=/run/dj-os-pendrive linha parte fs disco rotulo ja dir tipo
    salvos=""
    mkdir -p "$m"
    for linha in $(lsblk -nr -o NAME,TYPE,PKNAME | awk '$2=="part" {print $1":"$3}'); do
        IFS=: read -r parte disco <<<"$linha"
        # O tipo vem direto do disco (pelo Ventoy o lsblk mostra em branco).
        fs=$(blkid -p -o value -s TYPE "/dev/$parte" 2>/dev/null)
        case "$fs" in vfat|exfat|ntfs|ext2|ext3|ext4) ;; *) continue ;; esac
        [ "$disco" = "$pular" ] && continue
        [ "$(lsblk -dn -o TRAN "/dev/$disco" 2>/dev/null | tr -d ' ')" = usb ] || continue
        rotulo=$(blkid -p -o value -s LABEL "/dev/$parte" 2>/dev/null)
        ja=$(findmnt -n -o TARGET -S "/dev/$parte" | head -1)
        if [ -n "$ja" ]; then
            mount -o remount,rw "$ja" 2>/dev/null || continue
            dir=$ja
        else
            tipo=$fs; [ "$fs" = ntfs ] && tipo=ntfs3
            mount -t "$tipo" "/dev/$parte" "$m" 2>/dev/null || continue
            dir=$m
        fi
        if mkdir -p "$dir/$pasta" && cp "$arq" "$dir/$pasta/" 2>/dev/null; then
            salvos="$salvos\n  - pendrive ${rotulo:-$parte}, pasta $pasta"
        fi
        sync
        if [ -n "$ja" ]; then mount -o remount,ro "$ja" 2>/dev/null; else umount "$m"; fi
    done
}

# Junta em <pasta> os registros do DJ OS instalado neste computador (se houver): registro da
# sessão (programa e tela), mensagens do sistema dos últimos boots e a configuração. Lê o disco só
# para leitura. Devolve 1 se não achou o DJ OS instalado.
junta_instalado() {
    local R=$1 m=/run/dj-os-instalado dev d b achou=1
    mkdir -p "$R" "$m"
    for dev in $(lsblk -nr -o PATH,LABEL | awk '$2=="DJOS_DADOS" {print $1}'); do
        mount -o ro "$dev" "$m" 2>/dev/null || continue
        achou=0
        d=$R/instalado-$(basename "$dev")
        mkdir -p "$d"
        cp "$m"/dj/sessao.log* "$m"/dj/config.txt "$m"/sistema/temperatura.log* "$d/" 2>/dev/null
        ls -la "$m/dj" >"$d/arquivos.txt" 2>&1
        if [ -d "$m/sistema/journal" ]; then
            journalctl -D "$m/sistema/journal" --list-boots --no-pager >"$d/boots.txt" 2>&1
            for b in 0 -1 -2 -3; do
                journalctl -D "$m/sistema/journal" -b "$b" --no-pager -o short-monotonic >"$d/sistema-boot$b.txt" 2>&1 || rm -f "$d/sistema-boot$b.txt"
            done
        fi
        umount "$m"
    done
    return $achou
}

# Informações da máquina (discos, placa de vídeo e de som, mensagens do kernel).
info_maquina() {
    echo "== DJ OS $(cat /etc/dj-os-versao) - $(date)"
    echo "== boot"; cat /proc/cmdline
    echo "== discos"; lsblk -o NAME,SIZE,TYPE,FSTYPE,LABEL,TRAN,MODEL
    echo "== video"; lspci -nnk | grep -A3 -i -e vga -e display
    echo "== som"; lspci -nnk | grep -A3 -i audio
    echo "== saude dos discos (SMART)"
    for d in $(lsblk -dn -r -o NAME,TYPE | awk '$2=="disk" {print $1}'); do
        echo "-- /dev/$d"; smartctl -H -A -l error "/dev/$d" 2>&1 | tail -n +4
    done
    echo "== kernel"; dmesg | tail -n 150
}
