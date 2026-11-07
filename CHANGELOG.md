# Histórico de versões

O formato segue [Keep a Changelog](https://keepachangelog.com/pt-BR/1.1.0/) e a versão segue
*maior.menor.correção*. Cada montagem leva também a data e a hora (ex.: `0.1.0-20261029.1245`),
para diferenciar firmwares de teste da mesma versão.

## [Não lançado]

## [0.1.5] - 2026-11-06

Som mais aberto, waveform com as cores dos aparelhos de cabine, zoom pelo BROWSE e o DJ OS
Preparar novo (dividido no meio, com player e hot cues). Testado na cabine com a DDJ-REV1.

### Novo
- **DJ OS Preparar** repaginado, no visual preto do aparelho, dividido no meio:
  - à esquerda o **computador** (escolhe o disco: Músicas, pasta pessoal, HDs internos e
    externos; abre onde parou) e à direita o **pendrive** (com o botão **EJETAR**, para tirar com
    segurança). Em cada lado: pastas e músicas com BPM, tom e ✓ de analisada, "← Voltar", busca.
  - **Arrastar** músicas ou pastas do computador para o pendrive (para a pasta aberta ou em cima
    de uma pasta) copia, mantendo as subpastas, e já analisa; também pelo botão "Copiar para o
    pendrive". Música que já está lá não é copiada de novo; a cópia nunca deixa arquivo pela metade.
  - No pendrive: nova pasta, renomear (F2), mover (arrastando) e apagar (Delete, com confirmação).
    O que está no computador nunca é apagado.
  - **Player embaixo**: dois cliques numa música para ouvir; waveform da música inteira (clique
    para pular), play/pausa (espaço), **CUE** e **hot cues A–H** (clique marca ou vai; botão
    direito apaga). Os cues de música do pendrive vão direto para a análise do pendrive; os de
    música do computador vão junto quando ela for copiada.
  - Corrigido: o Preparar fechava sozinho ao escolher uma pasta (a janela de escolher pasta do
    sistema não abria).
- Lançamento automático: com várias versões publicadas juntas, a de número maior fica como
  "Latest".
- **Zoom da waveform pelo BROWSE**: segurar o BROWSE e girar aproxima/afasta a waveform (girar
  enquanto segura não abre a pesquisa). Em CONFIG → CONTROLADORA → "Girar o BROWSE na tela
  principal": ABRE O BROWSER (como antes) ou ZOOM DA WAVEFORM; no modo zoom o browser abre
  apertando o BROWSE ou com SHIFT + BROWSE.
- **Cores da waveform como nos aparelhos de cabine** (paleta AZUL/LARANJA): as três bandas
  empilhadas a partir do meio, creme (agudo) no centro, âmbar (médio) em volta e azul (grave) por
  fora, cada uma com a sua grossura.
- Cues: entre a análise do aparelho e a do pendrive, vale a gravada por último (cues marcados no
  Preparar depois de a música tocar no aparelho passam a valer).

### Corrigido
- **Som mais aberto no master e no fone**: o limitador de proteção tinha o teto em -1 dBFS e
  apertava em quase toda batida forte, mesmo com uma música só e tudo no meio (medido: até
  ~1,3 dB, 65% do tempo), achatando o som. Agora o teto é -0,2 dBFS e ele é mais suave (olha 4 ms
  à frente e volta em ~300 ms): uma música sozinha passa praticamente intacta, e ele só trabalha
  quando a soma realmente estouraria. O ganho automático sobe no máximo +3 dB (antes +6).
- Waveform sem o agudo no modo leve (placa de vídeo antiga): cortando grave e médio no EQ sobrava
  uma linha limpa. Agora as três bandas aparecem sempre, e o agudo ficou maior.

## [0.1.4] - 2026-11-03

O browser novo, preto, com a mesma lógica do browser de árvore, e o pendrive do rekordbox.

### Novo
- **Browser novo** em tela cheia (SHIFT + BROWSE), no visual dos aparelhos de cabine: fundo preto,
  pastas à esquerda, músicas da pasta à direita (com BPM e tom; o tom fica verde quando combina
  com a música que está tocando) e os 4 decks embaixo (tempo restante, BPM, tom e a música
  inteira com os hot cues). A lógica é a do browser de árvore: apertar o BROWSE alterna entre as
  pastas e as músicas, o LOAD abre a pasta ou carrega a música, segurar o BROWSE pesquisa. A barra
  de cima do sistema some enquanto ele está aberto. Também com o mouse (clique, dois cliques abre
  a pasta ou carrega a música, clicar num deck de baixo carrega nele, rodinha). Em CONFIG → TELA:
  voltar para o browser clássico, colunas de BPM e tom e o tamanho da letra.
- **Pendrive do rekordbox**: o DJ OS lê o banco do pendrive exportado pelo rekordbox. No browser
  aparece "📀 rekordbox" com todas as músicas e as playlists (com as pastas delas); nas pastas,
  nome, artista, BPM e tom vêm do rekordbox. Ao abrir uma música, a grade, o tom e os cues
  (memory cue, hot cues A-H e loop) são os do rekordbox, sem analisar de novo. O Preparar e o
  "Analisar todas" também usam. Só leitura: nada é gravado no pendrive.

## [0.1.3] - 2026-11-03

Versão alternativa (branch `browser-categorias`): o browser novo com categorias à esquerda
(ARTISTA, MÚSICAS, PLAYLIST, HISTÓRICO, PASTAS, FILA, GRAVAÇÕES), para quem prefere esse jeito.
Tem também o pendrive do rekordbox. A linha principal seguiu na 0.1.4.

## [0.1.2] - 2026-11-03

Slip, aba MIXER, volume da REV1, scratch mais liso e HD/SSD externo na biblioteca.

### Novo
- Aba **MIXER** nas configurações (inspirada nos mixers de clube):
  - modo do EQ: isolador (corta tudo no mínimo) ou EQ (até -26 dB);
  - curva do fader de canal: lenta, normal ou rápida (corte);
  - curva do crossfader (saiu da aba Controladora);
  - lado de cada deck no crossfader: A, B ou THRU (fora do crossfader);
  - fone em estéreo ou mono dividido (CUE no ouvido esquerdo, master no direito).
- **Slip** (CONFIG → CONTROLADORA, desligado de fábrica): scratch, backspin e loop "por cima" da
  música; ao soltar o prato ou sair do loop, ela volta onde estaria se tivesse seguido tocando.
  Uma linha tracejada na waveform mostra onde é; a etiqueta SLIP acende enquanto está valendo.

### Corrigido
- Som baixo na REV1 mesmo com tudo no máximo: o sistema agora deixa a saída da REV1 em 100%
  (no PipeWire e na placa) e registra como estava.
- Botão MASTER: o meio agora é 0 dB (antes -6 dB), e a metade de baixo também tem volume.
- Scratch e backspin dentro do loop: para trás, o som volta pelo fim do loop em vez de sair dele.
- Mão no prato: o som freia liso e fica mudo com a mão parada (antes chiava e deixava um zumbido);
  o scratch segue a mão sem o "serrilhado" digital e para exatamente onde a mão parou.
- HD e SSD externos (em case USB) não apareciam na biblioteca, só pendrives.
- Dois discos com o mesmo nome (ex.: "Novo volume" do Windows) aparecem os dois; a partição de
  boot (EFI) de um disco com sistema instalado não aparece mais.

## [0.1.1] - 2026-10-30

Correções do primeiro teste na máquina de verdade (notebook de 2011, i5-2430M, Intel HD 3000) e o
programa do computador de casa.

### Novo
- **DJ OS Preparar** (Windows e Linux): prepara o pendrive no computador de casa, como o rekordbox.
  As músicas chegam ao aparelho já analisadas (BPM, grade, tom, volume, waveform).
- Efeitos refeitos: reverb de placa (Dattorro/CAPS, como no Mixxx), eco que troca de tempo sem
  esticar e com repetições mais escuras, flanger mais fundo; sem estourar o volume.
- Divisões de batida 1/32 e 1/64 (e 32) nos efeitos: descendo o eco até 1/64 com ele soando, a
  voz fica "robotizada", como no VirtualDJ.
- Medidores de volume dos canais acendem na controladora.
- CONFIG → SISTEMA mostra a versão com a data do firmware; "Analisar todas as músicas".
- CONFIG → CONTROLADORA → "Registrar os botões" (diagnóstico).

### Corrigido
- Tela e controladora congelavam no meio do set (só o som seguia).
- O som da DDJ-REV1 caía e voltava sozinho: uma consulta ao PipeWire que falhava por um instante
  fechava a saída. Agora a troca de saída só vale depois de confirmada.
- O aparelho desligava sozinho por calor: controle de temperatura do processador, sem turbo, e a
  análise das músicas não roda mais em segundo plano (só no LOAD).
- Nomes com acento no pendrive (ç, ã, é) apareciam quebrados.
- Atualização de firmware mais leve (não esquenta) e com erro claro quando o arquivo está incompleto.
- Desligar no fim da instalação não trava se o pendrive for tirado antes da hora.

## [0.1.0] - 2026-10-29

Primeira versão funcional, em teste com a Pioneer DDJ-REV1.

### Sistema de DJ
- 4 decks com waveform de 3 bandas, cue estilo CDJ, 8 hotcues, loops, roll, beat jump, sync e
  keylock.
- Scratch com inércia (backspin segue o prato solto até ele parar).
- Mixer com EQ de 3 bandas, filtro, crossfader com 3 curvas e saída de fone (REV1: master 1-2,
  fone 3-4).
- Efeitos por canal sincronizados ao BPM (Echo, PingPong, Reverb, Flanger, Phaser, Wahwah, Noise,
  Cut), com divisão de batida de 1/16 a 16.
- Browser com árvore de pastas, pesquisa com teclado virtual e importação de pastas do pendrive
  com análise de BPM automática.
- Memória por música (BPM, cues, hotcues, loop).

### Som e conexões
- Saída direta pelo PipeWire; sem saída de som o sistema continua funcionando em silêncio.
- Caixa Bluetooth como master ou retorno, ao mesmo tempo que a REV1, com ajuste de atraso.
- Celular por Bluetooth: o sistema vira caixa de som e o som entra num deck ou no master.

### Aparelho (ISO)
- ISO para pendrive/Ventoy com "testar sem instalar", "instalar no disco" e modo de segurança.
- Firmware A/B assinado: atualização pelo pendrive com volta automática para a versão anterior.

### Também nesta versão
- Configurações em abas (estilo XDJ), com resumo de cada aba e navegação pelo BROWSE.
- Tela de inicialização com a logo do DJ OS; menu de boot escondido no aparelho instalado (Esc mostra).
- Botão power desliga com segurança; fechar a tampa ou teclas de suspender não fazem nada.
- Limitador no master e no fone: o som nunca passa de -1 dBFS (aviso "LIM" na barra de cima).
- Tom da música (key) no formato Camelot com cores, no browser e no deck; a análise automática
  completa o tom das músicas já analisadas sem perder os cues.
- Ganho automático: mede o volume de cada música (LUFS) e nivela em -9 LUFS (de -12 a +6 dB),
  com opção em CONFIG → ÁUDIO e o valor aplicado no deck ("AG").
- Gravar o set (botão GRAVAR ou Ctrl+R): WAV do master na pasta "records", no aparelho ou direto
  no pendrive (CONFIG → ÁUDIO → Gravar em); protegido contra falta de luz; copiar do aparelho
  para o pendrive pelo browser.
- Histórico da noite: música que toca 30 s com o fader aberto entra em "🕘 Tocadas hoje" (com a
  hora), ganha ✓ nas listas e vai para uma playlist M3U8 em ~/DJOS/historico.
- Ajuste da grade de batidas (tocar no BPM do deck): BPM ÷2/×2/±1/±0,1/±0,01, TAP, deslocar a
  grade (±1 ms, ±10 ms, ±½ batida) e "BATIDA AQUI"; vale na hora e fica salvo na música.
- Retomar depois de um travamento: se o DJ OS cair no meio do set, reabre com as músicas nos
  decks, no mesmo ponto.
- Catálogo de controladoras: reconhece pela porta MIDI e avisa as ainda não suportadas
  (guia em docs/controladoras.md).

### Correções
- Eco na caixa Bluetooth (o master ia duas vezes para ela).
- Scratch: o backspin segue o prato solto até ele parar.
- Celular por Bluetooth: pareamento sem pedir código e nome "DJ OS"; visibilidade com prazo.
- Nomes: acentos separados viravam quadradinho; número da faixa no nome do arquivo virava artista.
