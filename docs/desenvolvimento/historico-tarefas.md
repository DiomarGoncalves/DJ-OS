# Tarefas do DJ OS

## Resumo da noite (2026-10-28 → 29)

As 14 tarefas do modo automático estão feitas, compilando e com **58 testes automáticos passando**
(`cargo test --release`). Cada uma tem um ponto de salvamento no git (`git log --oneline`).

**Para testar com a REV1 hoje** (guia botão por botão em `docs/mapeamento-ddj-rev1.md`):
1. Ligar a REV1, abrir o sistema (`cargo run --release` dentro de `dj-os/`) e conferir: jog,
   pads e os 8 modos, alavanca e knob central dos efeitos (FX 1 aceso), BROWSE segurado (pesquisa),
   SHIFT + BROWSE (tela cheia), SHIFT + MASTER CUE (configurações), LEDs.
2. Saídas: com a REV1 ligada o sistema abre 4 canais (master 1-2, fone 3-4).
3. Pendrive: abrir pastas, pesquisar, SHIFT + LOAD numa pasta para salvar no sistema.
4. Ouvir efeitos, keylock e Bluetooth (parear pela tela de configurações).
5. Spotify: escolher "DJ OS" no app (precisa de internet e Premium).

**Problema da noite:** as pontes ALSA de áudio deste PC travavam (ver "Bloqueios"); resolvido com a
saída direta pelo PipeWire. O serviço de som do PC (pipewire-pulse) foi reiniciado uma vez, às 00:22.

Lista de trabalho para o modo automático (noite). Faço de cima para baixo.
Cada tarefa concluída vira um ponto de salvamento no git (`git log` mostra tudo).

Legenda: `[ ]` a fazer · `[x]` feito · `[!]` travou (motivo em "Bloqueios")

## Regras do modo automático

- Só mexo em `dj-os/` e em `~/DJOS/`. Nada fora disso.
- Sem `sudo` e sem instalar nada no sistema (ferramentas Rust ficam em `~/.cargo`).
- Compilo com prioridade baixa (`nice`) e no máximo 4 núcleos, para não travar o PC.
- Cada tarefa termina compilando sem erro e com os testes passando; senão volto a tarefa.
- Se uma tarefa travar, anoto em "Bloqueios" e passo para a próxima.
- Ritmo: uma tarefa por rodada, com pausa de 5 minutos entre as rodadas, para não estourar o limite de uso
  do plano. Melhor terminar menos tarefas e bem testadas do que muitas pela metade.
- Toda rodada termina com o sistema compilando e funcionando: de manhã o PC vai para o trabalho do
  Diomar para testes, então nada pode ficar quebrado no meio.
- Não envio nada para a internet (sem `git push`, sem publicar).
- Não mudo o Wi-Fi nem o Bluetooth deste PC: de noite só listo redes e aparelhos; conectar e parear
  de verdade fica para testar junto com você.
- Nada do VirtualDJ é aberto, copiado ou analisado: só o seu mapeamento, como lista de funções.

## Prioridade 1: o que mais importa (efeitos, browser, tela)

- [x] **1. Consumo de CPU.** Medido em 2026-10-29 neste PC (Intel Iris Xe), modo demo com 2 decks
  tocando, echo ligado, loop ativo e tela a 60 quadros: **14,8% de um núcleo** no total (áudio 7%,
  tela 6,5%, PipeWire 0,8%). Meta (30%) cumprida. Ajustes: busca da REV1 reaproveita um só cliente MIDI;
  `DJOS_AUDIO` agora só usa o dispositivo pedido (antes caía em silêncio na saída padrão);
  `--listar-audio` mostra as saídas; `DJOS_MUDO=1` testa sem som. Falta medir na máquina final (HD 3000).
- [x] **2. Testes automáticos do motor.** 21 testes em `src/engine/tests.rs`, todos passando
  (`cargo test --release`, 0,6 s): cue CDJ, preview com PLAY, hotcues, fim da música, pitch e faixa,
  autoloop na grade, metade/dobro/reloop, loop in/out, roll (volta onde estaria), beat jump, EQ e filtro
  neutros no centro, EQ no mínimo corta, fader, os 8 efeitos em todas as batidas sem estourar, cauda de
  echo/ping pong/reverb que continua e some, efeitos desligados não mexem no som, crossfader, saídas
  master (1-2) e fone (3-4) separadas, sync de BPM e fase com LED, comandos inválidos não quebram.
- [x] **3. Efeitos do jeito da REV1** (feito em 2026-10-29):
  - Um efeito por deck (Echo de início). FX 1 (com ou sem SHIFT) mostra a lista de efeitos na tela e o
    efeito escolhido entra no deck depois de 1 s sem clicar; girar o BROWSE segurando um FX também anda
    pela lista. FX 2 / FX 3 diminuem / aumentam a batida (1/16 … 3/4, 1, 1 1/2, 2 … 16), com aviso na tela.
  - Alavanca (0x50/0x66/0x47-0x49) liga/desliga; knob central (CC 02/04/06, 14 bits) é só a força.
    Tirado o filtro que subia junto; ficou só um corte fixo de grave no eco (110 Hz) e no reverb (160 Hz).
  - Tela: ON/OFF, nome, batida, barra e % da força; durante a escolha mostra a lista com o destaque.
  - LED do FX 1 fica aceso (pisca com o efeito ligado) para o knob e a alavanca mandarem os códigos do
    "slot 1". **Conferir com a REV1 na mão** que o knob central responde assim.
  - Teclado: F1/F5 alavanca, F2-F3/F6-F7 batida, F4/F8 troca o efeito. Testes novos: 24 no total.
  - De quebra: o áudio agora abre em segundo plano (a tela nunca fica presa) e aparece "ÁUDIO PARADO"
    em vermelho se o motor não rodar; sem áudio a tela mostra "--" em vez de números sem sentido.
- [x] **4. Browser igual ao VirtualDJ e pesquisa estilo XDJ** (feito em 2026-10-29):
  - Árvore de pastas: unidades (pendrive/SSD em `/media/<usuário>`, músicas internas, ~/Música);
    partições do PC (`/mnt`, outros usuários) não aparecem mais. LOAD numa pasta abre/fecha as
    subpastas; sem subpastas, entra nas músicas.
  - Toque curto no BROWSE (ao soltar) alterna pastas/músicas; SHIFT + BROWSE abre/fecha a tela cheia.
  - Segurar o BROWSE 0,5 s abre a pesquisa em tela cheia com o teclado (QWERTY grande, dá para tocar):
    girar escolhe a tecla, apertar digita; pads 1-8 = cima, baixo, esq, dir, confirma, apaga, espaço,
    fecha (só com o teclado aberto; fora dele os pads continuam normais). Mostra ao vivo quantas
    músicas a busca acha. ENTER: resultados viram o item "🔍 termo (N)" no topo, com a lista em foco.
  - Busca em todas as unidades (índice em segundo plano, refeito quando troca o pendrive), sem
    diferenciar maiúsculas e acentos, palavras em qualquer ordem, até 500 resultados.
  - LOAD numa música carrega e fecha a tela cheia; com o teclado aberto o LOAD não faz nada.
  - Teclado do PC: Ctrl+F abre; digitar, Backspace, setas, Enter e Esc funcionam.
  - Testes novos: árvore, pesquisa (acento, subpasta, ordem das palavras) e teclado — 31 no total.
  - **Conferir com a REV1:** o "segurar" depende da REV1 mandar a mensagem de soltar o BROWSE
    (a lista oficial diz que manda: OFF=0x00).
- [x] **5. Tela principal no estilo XDJ-RR** (feito em 2026-10-29):
  - Waveforms dos dois decks grandes e empilhadas (3 cores, grade com tempo forte em vermelho, cues,
    loop destacado), zoom em 5 níveis (2, 4, 6, 10, 16 s): tocar na waveform ou +/- no teclado.
  - Coluna de efeito ao lado de cada waveform: ON/OFF (tocar liga), nome (tocar troca), batida grande
    com − e + e a barra de força.
  - Informações de cada deck: número, título, artista, BPM, pitch e faixa, tempo restante (pisca nos
    últimos 30 s), SYNC/VINIL/LOOP/ROLL/TOM e a visão geral da música com cues.
  - Pads embaixo: modo atual (tocar no nome troca o modo) e o estado de cada pad (cores dos hotcues,
    loop/roll ativo, nome dos samples). Tocar num pad faz o mesmo que o pad da REV1 (mesma função
    `pad_action`, segurar vale para hotcue, roll e trans).
  - A biblioteca saiu da tela principal (como na XDJ): girar o BROWSE ou as setas abrem a lista em
    tela cheia; LOAD volta para esta tela.
- [x] **6. Teste do mapeamento** (feito em 2026-10-29): 12 testes em `src/midi/tests.rs` simulam as
  mensagens da lista MIDI oficial e conferem o comando gerado: transporte dos 4 decks (inclui Note Off),
  loops, deck select e camadas, jog (scratch/bend/busca), modo Tracking voltando ao cue, valores de
  14 bits (tempo, fader, EQs na banda certa, mixer, filtros), PFL/MASTER CUE, efeitos por lado e por
  camada, os 8 botões de modo, os pads de cada modo (com SHIFT), pads virando setas com o teclado
  aberto, BROWSE curto/segurado/SHIFT, e os LEDs. Tabela botão → função em `docs/mapeamento-ddj-rev1.md`.
  43 testes no total.
- [x] **7. Memória por música** (feito em 2026-10-29): `src/store.rs` guarda em `~/DJOS/dados/`
  um arquivo por música (chave = nome + tamanho + data, vale mesmo se o pendrive montar em outro lugar)
  com BPM, grade, waveform, cue, hotcues e loop. Na primeira abertura a análise é salva; nas próximas
  a música abre sem analisar (teste: 98 ms → 35 ms) e volta com cue, hotcues e loop (o loop fica
  desligado, pronto para o RELOOP). Cue/hotcue/loop mexidos são salvos 1 s depois, em segundo plano,
  gravando num temporário e trocando (desligar no meio não corrompe). Testes: formato, arquivo
  estragado, chave e restauração no motor — 47 no total.
- [x] **8. Músicas internas e análise automática** (feito em 2026-10-29, `src/import.rs`):
  - "🏠 Músicas internas" (`~/DJOS/musicas`) aparece sempre no browser, mesmo sem pendrive.
  - Importar: SHIFT + LOAD numa pasta do pendrive/SSD, o botão "SALVAR NO SISTEMA" (toque) ou Ctrl+S.
    Copia em segundo plano com prioridade baixa, mantendo as subpastas, sem duplicar (mesmo nome e
    tamanho = pula), conferindo o espaço antes (deixa 1 GB de folga) e com barra de progresso. Cada
    arquivo é copiado como temporário e renomeado no fim (cópia interrompida não deixa lixo).
  - Análise automática: thread de prioridade baixa analisa toda música do índice sem análise salva
    (uma por vez, com folga entre elas). Testado: a música sem análise foi analisada sozinha.
  - BPM na lista de músicas e nos resultados da pesquisa (atualiza quando a análise termina).
  - Apagar: botão "APAGAR" (toque) ou Delete, só dentro das músicas internas, com confirmação
    (apertar de novo em 4 s).
  - **Falta testar com um pendrive de verdade** (aqui não há pendrive montado). 48 testes no total.

## Prioridade 2: funções

- [x] **9. Saídas de áudio pelo PipeWire** (feito em 2026-10-29, adiantada por causa do bloqueio do
  áudio). `src/pw_out.rs` fala direto com a `libpipewire-0.3.so.0` (carregada em tempo de execução, sem
  pacote de desenvolvimento). Aparece no PipeWire como o nó "dj-os", F32 48 kHz, latência 256
  amostras (5,3 ms). Testado neste PC: formato conferido no `pw-dump` (F32LE, FL/FR), ligado à saída
  padrão, tocando em menos de 1 s nas 3 aberturas (antes: 5 s ou nunca). Com a REV1 ligada abre 4 canais
  (AUX0-3: 1-2 master, 3-4 fone) apontados para ela — **falta testar com a REV1 na mão**. As pontes ALSA
  ficam de reserva (ou `DJOS_SAIDA=alsa`, ou `DJOS_AUDIO=<dispositivo>`). Teste do formato: 25 testes no total.
- [x] **10. Bluetooth simultâneo** (feito em 2026-10-29, `src/bluetooth.rs`):
  - Botão na barra de cima (tocar): desligado → MASTER → RETORNO. O master vai sem atraso e o
    retorno é a mistura do fone. A cada 5 s o sistema procura uma caixa conectada ("bluez_output" no
    PipeWire) e abre/fecha o fluxo "dj-os-bluetooth" sozinho.
  - Relógios diferentes (REV1 × caixa): fila de 100 ms com correção de velocidade de até 0,5% para
    nunca faltar nem sobrar áudio.
  - Atraso do master com fio (saídas 1-2, até 500 ms, suave) para alinhar com a caixa quando ela é
    master no mesmo ambiente; o fone não atrasa. Por enquanto em `DJOS_BT_ATRASO_MS` (o ajuste vai para
    a tela de configurações, tarefa 12). `DJOS_BT=master|retorno` liga ao abrir.
  - Testado: os dois fluxos rodando juntos no PipeWire (com destino de teste); teste do motor confere
    que o Bluetooth recebe todo o áudio e que o fio atrasa exatamente o pedido. 50 testes.
  - **Falta com você:** parear uma caixa de verdade e ouvir (o pareamento entra na tarefa 12).
- [x] **11. Keylock** (feito em 2026-10-29, `src/engine/keylock.rs`): WSOLA — grãos de 43 ms na
  velocidade normal, encaixados (busca de ±11 ms no sinal reduzido) na continuação do grão anterior.
  Liga/desliga por deck tocando no "KEYLOCK" do painel do deck ou com Q / P (`DJOS_KEYLOCK=1` liga em
  todos ao abrir). Fica ativo só tocando, sem scratch, com pitch fora do zero e sem o tom dos pads
  Tracking; loops, roll e cues seguem funcionando. Teste: +8% de pitch mantém 220 Hz (sem keylock vai
  a 237,6 Hz), o andamento acelera exatamente 8% e o volume não muda. CPU: +1,7% de um núcleo por deck
  com keylock (medido aqui; medir na máquina final). 51 testes.
- [x] **12. Tela de configurações dentro do sistema** (feito em 2026-10-29, `src/settings.rs`):
  - Abre com SHIFT + MASTER CUE (botão livre no mapeamento), o botão CONFIG da barra de cima ou F10.
    Girar o BROWSE escolhe, apertar muda; dá para tocar nos itens; setas/Enter/Esc no teclado.
  - Wi-Fi: procura redes (`nmcli`, só leitura), escolher uma rede abre o teclado de senha (agora com
    minúsculas "Aa" e símbolos) e ENTER conecta (`nmcli dev wifi connect`).
  - Bluetooth: modo (desligado/master/retorno), atraso do master com fio (0-500 ms), procurar caixas
    (`bluetoothctl scan` 8 s) e parear + confiar + conectar.
  - Áudio: mostra a saída atual. Controladora: jog VINIL/CDJ ao ligar, faixa de pitch (8/16/50%),
    curva do crossfader (suave, corte para scratch, potência constante — nova no motor), keylock ao
    carregar. Visual: 3 paletas de cores da waveform e tamanho da tela (100/115/130%). Sobre: versão.
  - Tudo salvo em `~/DJOS/config.txt` e aplicado ao abrir o sistema. 54 testes.
  - **Falta testar com você:** conectar num Wi-Fi e parear uma caixa (de noite só listei, pela regra).
- [x] **13. Spotify Connect** (feito em 2026-10-29, `src/spotify.rs`): librespot 0.8.0 instalado em
  `~/.cargo/bin` (compilado sem OpenSSL e sem bibliotecas de áudio). O sistema abre o librespot com a
  saída "pipe", converte 44,1 → 48 kHz e mistura no master como som ambiente; abaixa sozinho (~1 s)
  quando um deck toca com o fader aberto (opção). Configurações: ligar/desligar, volume, abaixar,
  estado. Credenciais ficam em `~/DJOS/spotify`. Se o sistema fechar, o librespot fecha junto.
  Testado: o librespot sobe com o sistema e anuncia "DJ OS" na rede (mDNS porta 5353).
  `DJOS_SEM_SPOTIFY=1` desliga ao abrir. 57 testes.
  - **Falta com você:** escolher "DJ OS" no app do Spotify (precisa de Premium) e ouvir.
  - **ISO:** levar o binário do librespot junto (ou compilar na montagem da ISO).

## Prioridade 3: desempenho

- [x] **14. Modo leve para a Intel HD Graphics 3000** (feito em 2026-10-29): ao abrir, o sistema lê o
  nome da placa de vídeo; Intel HD 2000/3000 (Sandy Bridge), Ironlake ou driver sem aceleração
  (llvmpipe) ligam o modo leve sozinho. Configurações → Visual: "Modo leve" AUTOMÁTICO / LIGADO /
  DESLIGADO, mostrando a placa detectada. O modo leve limita a tela a 30 quadros/s (como o skinFPS=30
  do VirtualDJ) e desenha a waveform em colunas de 2 px com 2 camadas e a visão geral com 1 camada.
  Medido aqui: thread da tela de 6,8% → 2,3% de um núcleo; total de 11,5% → 5,8%. Visual conferido
  por captura. `DJOS_MODO_LEVE=1` força ligado. 58 testes. **Medir na máquina final.**

## Revisão com o Diomar (começada em 2026-10-29)

Depois do primeiro teste da ISO no VirtualBox: "funcional, mas a experiência não está das melhores".
Ordem combinada: configurações → tela principal → browser e pesquisa → instalar e firmware.

- [x] Tirar o Spotify Connect (som de fora só pelo celular por Bluetooth).
- [x] Teclado brasileiro (ABNT2) na ISO.
- [x] Sem placa de som o motor parava (play e hotcue não faziam nada): agora roda num relógio
      silencioso e passa para a REV1 quando ela aparece. Era o caso do VirtualBox dele.
- [ ] **Configurações em abas à esquerda (estilo XDJ):** Áudio, Bluetooth, Celular, Wi-Fi,
      Controladora, Tela, Sistema. BROWSE anda nas abas e entra nelas.
- [ ] Tela principal: revisar com ele o que falta ou está estranho.
- [ ] Browser e pesquisa: revisar com ele.
- [ ] ISO etapa 2 (instalar no disco) e etapa 3 (firmware A/B com assinatura).

## Fazer junto com o Diomar (fora do modo automático)

Estas partes ficam para o final e são feitas com você presente. As ferramentas já estão instaladas
(mmdebstrap, xorriso, QEMU, OVMF), em 2026-10-28.

- [ ] **ISO instaladora.** Um arquivo `.iso` que funciona em pendrive bootável ou no Ventoy. Ao ligar por
  ele, aparece uma tela simples: você escolhe o disco onde instalar e o resto é automático (apaga o disco
  escolhido, instala o Linux mínimo e o sistema, e configura para ligar direto no DJ OS).
  Debian 13 mínimo montado sem `sudo` (mmdebstrap), BIOS e UEFI, testado antes numa máquina virtual.
- [ ] **Sistema no estilo firmware.** O usuário não vê Linux: liga e cai direto no DJ OS, como um
  aparelho. Sistema base protegido contra escrita acidental; músicas, cues e configurações ficam numa
  área separada que sobrevive às atualizações.
- [ ] **Atualizar firmware pelo pendrive.** Em Configurações → "Atualizar firmware": colocar um pendrive
  só com o arquivo de firmware, o sistema acha sozinho, confere se o arquivo está íntegro e é oficial
  (assinatura digital, para não instalar arquivo corrompido ou falsificado), instala e reinicia.
  Guarda a versão anterior: se a nova não ligar, volta sozinho para a anterior.

## Bloqueios

- ~~**Áudio pelas pontes ALSA (2026-10-29).**~~ Resolvido pela tarefa 9 (PipeWire direto). Neste PC, abrir o áudio pelos dispositivos ALSA "pipewire"
  e "default" às vezes leva 5 s para começar e às vezes nunca começa; a ponte "pulse" funcionou na hora
  e depois também passou a travar. Numa sequência de testes o serviço pipewire-pulse do usuário parou de
  responder e foi reiniciado (`systemctl --user restart pipewire-pulse`, às 00:22) — voltou normal.
  Solução: tarefa 9 (falar direto com o PipeWire). Até lá o sistema não trava a tela e avisa
  "ÁUDIO PARADO". Testes de áudio passam a ser poucos e espaçados para não derrubar o som do PC.
- ~~Unidades estranhas no browser~~ — resolvido na tarefa 4.

## Decisões já tomadas (2026-10-28)

- **Nome:** DJ OS por enquanto (vai mudar depois).
- **Browser e pesquisa:** como descrito na tarefa 4 (explicação do Diomar em 2026-10-28).
- **Efeitos:** um por deck; FX 1 troca o efeito, FX 2/3 mudam a batida, alavanca liga, knob central
  é a força; o "corte" é a divisão de batida, não filtro (tarefa 3).
- **Configurações:** Wi-Fi, Bluetooth, saídas e visual ficam todos dentro do sistema.
- **Tela:** ainda não é touch, mas vai ganhar um adaptador touch. Tudo funciona pela controladora;
  botões e teclas da tela já ficam grandes o bastante para o toque (mínimo 44 px).
- **Memória da máquina final:** entre 6 e 10 GB. Áudio na memória em formato normal (sem modo compacto);
  medir o uso com 4 decks + samples e anotar.
- **Pads:** fica a minha escolha (abaixo); você ajusta testando com a REV1.

## Decisões que esperam você

- **Internet na máquina final:** ela não tem Wi-Fi. Para Wi-Fi e Spotify, precisa de um adaptador Wi-Fi
  USB (ver se funciona no Linux antes de comprar) ou cabo de rede.

- **Ajuste fino dos pads** quando testar com a REV1. Hoje:
  - Tracking: pads tocam a partir do cue nas notas 0, +2, +4, +5, +7, +9, +11, +12 (SHIFT = uma oitava abaixo);
  - Trans: corta o som no ritmo de 1, 1/2, 1/4, 1/8, 1/16, 2/3, 1/3, 1/6 batida enquanto segura;
  - Beat Jump: ←1 1→ ←2 2→ ←4 4→ ←8 8→ (SHIFT = 4 vezes mais);
  - Roll: 1/32 até 4 batidas.
- **Processador da máquina final** (a tela é 1366x768, a placa de vídeo é Intel HD 3000, memória 6-10 GB).
- **Testar com a REV1 na mão** e ouvir o som: só você consegue.
