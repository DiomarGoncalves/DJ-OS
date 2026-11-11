#!/bin/bash
# Mede quanto de um núcleo cada thread do DJ OS gasta tocando 2 decks (modo demo, sem som).
# Serve para comparar antes e depois de uma mudança: o DJ OS tem que rodar em computador antigo.
#
#   ferramentas/medir-cpu.sh [binário] [VAR=valor ...]
#
# - Sem binário: target/release/dj-os.
# - Precisa de 2 músicas em ~/DJOS/musicas (ou HOME=<pasta de teste> na frente do comando).
# - Tudo fica preso num núcleo só (taskset): sem isso o resultado varia até 2× de uma rodada
#   para outra (núcleos diferentes, frequência do processador). Rode 2 vezes cada versão.
# - As threads são casadas pelo número (não pelo nome): ordenar pelo texto mistura as linhas.
BIN="${1:-$(dirname "$0")/../target/release/dj-os}"
[ $# -gt 0 ] && shift
TMP=$(mktemp -d)
taskset -c "${NUCLEO:-2}" env DJOS_MUDO=1 DJOS_DEMO=1 "$@" "$BIN" >/dev/null 2>&1 &
PID=$!
ler() {
    for t in /proc/$PID/task/*; do
        set -- $(sed 's/.*) //' "$t/stat" 2>/dev/null)
        echo "$(basename "$t") $(tr ' ' '_' < "$t/comm" 2>/dev/null) $(( ${12:-0} + ${13:-0} ))"
    done | sort -k1,1
}
sleep "${ESPERA:-12}"; ler > "$TMP/a"; sleep 30; ler > "$TMP/b"; kill $PID 2>/dev/null
join "$TMP/a" "$TMP/b" | awk -v hz="$(getconf CLK_TCK)" '
    { d = ($5 - $3) / hz / 30 * 100; tot += d; n[$2] += d }
    END { for (k in n) if (n[k] >= 0.1) printf "  %-16s %5.1f%%\n", k, n[k]; printf "  %-16s %5.1f%%\n", "TOTAL", tot }' | sort -k2 -n -r
rm -rf "$TMP"
