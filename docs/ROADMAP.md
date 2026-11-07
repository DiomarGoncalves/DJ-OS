# Ideias para depois

## DJ OS para PC (combinado em 2026-11-07)
Virar um programa de DJ para qualquer computador (como VirtualDJ, rekordbox, Mixxx), além do
aparelho. Linux + Windows (Mac depois, sem Mac para testar). Etapas, cada uma funcionando antes
da próxima:

1. ~~**Modo PC**~~ (feito): no PC o DJ OS não mexe em Wi-Fi, Bluetooth, firmware nem desliga;
   janela normal maximizada; compila para Windows; AppImage e .exe nas Releases. Falta: escolher
   a saída de som (placa ou controladora) em CONFIG → ÁUDIO; registro em arquivo no Windows.
2. **Mapeamento por arquivo**: cada controladora num arquivo (MIDI → ação, LEDs, jog), com MIDI
   learn; a REV1 vira o primeiro arquivo. Escolher a controladora em CONFIG.
3. **Controladoras novas** pela documentação oficial pública (MIDI Message List de cada modelo):
   Pioneer DDJ-400/FLX4, SB3/FLX6/SR2/1000; CDJs/XDJs no modo USB (CDJ-2000NXS2, CDJ-3000,
   XDJ-RX/XZ); Numark, Hercules (Inpulse), Denon, Reloop. Cada uma "em teste" até alguém com o
   aparelho confirmar. Os mapeamentos do Mixxx (GPL) servem de referência.
4. **Skins editáveis por arquivo** (como no VirtualDJ): posição e tamanho de cada parte da tela,
   cores e fontes; vários prontos (XDJ preto, claro, 2/4 decks); a pessoa edita e compartilha.

## Próxima atualização (combinado em 2026-11-03)
0. **Qualidade do som (fone e master)** — prioridade. Ele comparou a mesma música no PC e no
   aparelho e o aparelho soa pior ("não é aquele áudio gostoso"). Suspeitos, medir um a um com WAVs
   para ele ouvir antes de mudar: ganho automático em -9 LUFS + limitador apertando o tempo todo
   (som achatado); conversão de taxa 44,1 → 48 kHz com interpolação linear (perde brilho e
   cria aliasing); keylock ligado; caminho do fone. Comparar com o arquivo original (bit a bit com
   pitch 0, EQ no meio e ganho automático desligado).
1. ~~Zoom da waveform pelo BROWSE~~ (feito, firmware de 2026-11-04), só com a controladora:
   - CONFIG → CONTROLADORA → "Girar o BROWSE na tela principal": ABRE O BROWSER (como hoje) ou
     ZOOM DA WAVEFORM. No modo zoom, o browser abre pelo SHIFT + BROWSE (ou apertando o BROWSE).
   - Segurar o BROWSE e girar: zoom da waveform nos dois modos (girar enquanto segura não abre a
     pesquisa).
2. **Preparar, etapa 2**: player com a waveform (play/pause, clicar para pular), hot cues A–H e
   memory cue marcados no PC e gravados no pendrive; no aparelho vale o que foi marcado por último
   (hoje a cópia do próprio aparelho ganha da do pendrive).
3. ~~Cores da waveform como as das XDJ~~ (feito, firmware de 2026-11-04) (ele mandou prints da XDJ-AZ): três camadas espelhadas em
   volta do meio, azul forte por fora (graves), laranja/âmbar no meio (médios) e creme quase branco
   no centro (agudos), em fundo preto; o que já tocou na visão geral fica mais apagado. Ajustar a
   paleta AZUL/LARANJA (cores e o jeito de empilhar as bandas) na tela principal, nos decks do
   browser e no Preparar.
4. Sistema de baixar músicas (o dele) dentro do Preparar: fica para depois.

## Feito em 2026-11-03
Pendrive do rekordbox (0.1.4), browser novo preto com a lógica do de árvore (0.1.4; a versão com
categorias ficou como alternativa na 0.1.3), Preparar repaginado (pastas do PC e do pendrive, cópia
arrastando, organizar o pendrive). Android e Raspberry Pi: deixados de lado (só desktop).

## Combinado em 2026-10-31
Regras: tudo que muda o jeito de tocar vira opção em CONFIG (liga/desliga); toda tela continua
usável só pela controladora, sem toque e sem mouse.

1. **Slip mode** (opção em CONFIG): scratch, loop, backspin ou prato segurado e, ao soltar, a
   música volta onde estaria se tivesse seguido tocando.
2. **Pendrive do rekordbox no DJ OS** (só leitura): músicas, playlists, BPM, grade, tom, cues e
   hotcues do `export.pdb` + arquivos ANLZ, pelo formato documentado publicamente pela comunidade
   (Deep Symmetry / crate-digger). O banco criptografado novo (Device Library Plus) fica de fora.
3. **Playlists e Tag List** (fila de próximas, como nas CDJs).
4. **Preparar de verdade**: biblioteca, playlists, editar cues e grade, e exportar também no formato
   do rekordbox (o pendrive sai pronto para a CDJ). Continua gravando o `.dxj` (waveform de 3 bandas,
   LUFS); os pendrives antigos continuam funcionando. Testar no rekordbox e numa CDJ de verdade.
5. **Visual do mixer e das telas** no estilo dos aparelhos de clube (ele vai mandar prints).
6. Depois: "o que combina com essa" (BPM + tom), vinhetas no sampler, lista do set para postar.


Prioridade agora: o sistema 100% redondo com a DDJ-REV1. Estas ficam anotadas para quando o
sistema estiver consolidado.

## Outras controladoras (DDJ-400, DDJ-FLX4, …)
- ✅ Catálogo de controladoras (`MODELS` em `src/midi/mod.rs`): cada uma é reconhecida pelo nome
  da porta MIDI e tem o seu mapeamento; as desconhecidas aparecem como "ainda não suportada".
  Guia em [controladoras.md](controladoras.md).
- Falta: os mapeamentos em si (DDJ-400, FLX4…), com a lista MIDI de cada uma e alguém para testar.
- Depois: um assistente de "aprender" (aperte o botão na controladora, escolha a ação na tela)
  para mapear controladoras novas sem programar.

## Caixa Bluetooth e celular ao mesmo tempo
- Hoje, conectar a caixa derruba o celular (e vice-versa). Investigar com o registro do BlueZ no
  momento da conexão. Alternativa garantida: um segundo adaptador Bluetooth USB (um recebe do
  celular, o outro manda para a caixa).

## Outras plataformas
- **Raspberry Pi / ARM (aarch64):** o programa em Rust compila para ARM; falta montar a imagem
  (raiz Ubuntu/Debian arm64 e o boot próprio do Raspberry, que não usa GRUB) e medir o desempenho
  do áudio e da tela.
- Android e Raspberry Pi ficaram de lado (2026-10-31): o foco é desktop (ISO + Preparar no
  Windows/Linux). Riscos anotados: no Android a REV1 pode abrir só 2 canais (sem CUE) e o atraso do
  som; no Pi precisa Pi 5 + fonte de 27 W + cooler e uma imagem própria (sem Ventoy).

## Recursos de DJ (sugeridos)
- Tom da música (key) com cores para mixagem harmônica.
- Gravar o set no pendrive; histórico do que tocou.
- Ajuste da grade de batida; ganho automático; limitador no master.
