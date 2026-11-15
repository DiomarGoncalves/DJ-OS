#!/bin/bash
# Roda um comando como se fosse no notebook antigo da festa (i5-2430M de 2011, 2 núcleos e 4
# threads a 2,4 GHz, sem turbo), para achar travadas e atrasos antes do DJ.
#
#   ferramentas/lento.sh <comando...>
#   ferramentas/lento.sh target/release/dj-os
#   ferramentas/lento.sh cargo test --release -p dj-os som_aguenta_o_show -- --nocapture
#
# Como: o comando fica num núcleo só (taskset) com 30% da força dele (CPUQuota do systemd, em
# fatias de 5 ms para não dar soluços que o notebook não teria): uma thread do i5 antigo faz ~1/3
# do que uma do i5 de 12ª geração. É o pior caso (lá são 4 threads; aqui tudo divide uma).
# NUCLEOS=<lista> e FORCA=<%> mudam (ex.: NUCLEOS=0-3 FORCA=115 imita as 4 threads somadas).
# É uma aproximação: não imita a memória, o disco nem a penalidade de números denormais do
# processador antigo (essa lá é maior).
[ $# -gt 0 ] || { sed -n 2,15p "$0"; exit 1; }
export DJOS_LENTO=1
exec systemd-run --user --scope --quiet -p CPUQuota="${FORCA:-30}%" -p CPUQuotaPeriodSec=5ms \
    taskset -c "${NUCLEOS:-2}" "$@"
