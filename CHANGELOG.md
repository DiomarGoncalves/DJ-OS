# Histórico de versões

O formato segue [Keep a Changelog](https://keepachangelog.com/pt-BR/1.1.0/) e a versão segue
*maior.menor.correção*. Cada montagem leva também a data e a hora (ex.: `0.1.0-20261029.1245`),
para diferenciar firmwares de teste da mesma versão.

## [Não lançado]

### Novo
- **DJ OS para PC (Linux e Windows):** o DJ OS como programa de computador, com a controladora no
  USB. No PC ele não mexe em Wi-Fi, Bluetooth, firmware nem desliga o computador (essas partes
  somem); janela normal, maximizada. Nas Releases: `DJ-OS-PC-….AppImage` e `DJ-OS-PC-….exe`.
- **Controladoras por arquivo** (`.djosmap`, [docs/mapeamento.md](docs/mapeamento.md)): uma linha
  por botão, knob, fader e LED, sem programar. Vêm 21 controladoras **em teste**: Pioneer DDJ-400,
  FLX4, SB3, FLX6, SR2, 1000; CDJ-2000NXS2, CDJ-3000, CDJ-900NXS, XDJ-RX2, XDJ-RX3, XDJ-XZ;
  Numark Mixtrack Pro 3, Platinum FX, Party Mix; Hercules Inpulse 200/300/500; Denon MC7000,
  MC6000MK2; Reloop Beatmix 4. Os seus em `~/DJOS/mapeamentos/`.
- **MIDI learn:** CONFIG → CONTROLADORA → escolha a função e o deck, "Ensinar agora" e aperte o
  botão. Vale por cima do mapeamento (também na REV1); dá para apagar.
- **Saída do som** em CONFIG → ÁUDIO: automática (a controladora, de qualquer marca, com master
  1-2 e fone 3-4; senão a placa do computador) ou escolhida na lista.
- **Skins** (`.djosskin`, [docs/skins.md](docs/skins.md)) em CONFIG → TELA: cores de tudo e o
  layout da tela principal. Vêm Preta, Clara, Neon e Waveform grande; as suas em `~/DJOS/skins/`.
- **Funções das CDJs** (no aparelho e no PC):
  - **QUANTIZE** (CONFIG → CONTROLADORA, desligado de fábrica): cue, hot cue e loop in/out caem na
    batida; pular para um hot cue tocando mantém a fase (a batida não atravessa).
  - **Trava do LOAD**: com o deck tocando no ar (fader aberto), o LOAD não troca a música por
    engano. Abaixe o fader ou pare o deck. Dá para desligar.
  - **Aviso de fim da música**: nos últimos 30 s (ou 45/60 s, ou desligado) o painel do deck e o
    tempo restante piscam em vermelho.
  - **Medidor de fase** em cima de cada waveform: as 4 batidas do compasso, a de agora acesa, e
    onde está o outro deck ("FASE +0.00" em verde = batendo juntos).
  - **Contagem até o próximo cue** (compassos.batidas até o próximo hot cue ou CUE à frente).
  - **REVERSE** e **freio** (a música para devagar como toca-discos): pelos mapeamentos, MIDI learn
    e teclado (C/J reverse, V/H freio, Ctrl+U quantize). Com o slip ligado, o reverse vira
    "censor": ao desligar, a música volta onde estaria.
  - **Busca pela agulha:** tocar na música inteira (a mini-onda embaixo) pula para aquele ponto
    (com o deck no ar não pula, para não estragar o set).
  - MIDI learn com mais funções: hot cues E–H, loop in/out, reloop, slip, reverse, freio e quantize.
- **Aba FUNÇÕES** (a primeira de CONFIG: SHIFT + MASTER CUE e o BROWSE): tudo que antes só dava
  para fazer com toque ou mouse agora se faz pela controladora. Automix, gravar o set, filtro da
  lista, salvar a fila como playlist e, para cada deck da frente: casar o tom, tom +1/-1, consertar
  o BPM da grade (× 1,5, × 2/3, × 2, ÷ 2), "batida aqui" e reverse.
- **Música longa mostra o andamento ao carregar** ("Carregando Set (45%)…", depois
  "(analisando)"): um set de uma hora leva mais de um minuto num computador antigo e parecia
  travado. Com outra música tocando no deck, aparece uma faixa laranja na waveform.
- **Inspirado no VirtualDJ** (no jeito de usar; nada do programa deles):
  - **Automix** (botão AUTOMIX em cima, Ctrl+A ou botão ensinado): toca a fila (SHIFT + LOAD) ou
    a lista aberta a partir da música selecionada; a próxima entra no BPM e na batida da que está
    tocando e os faders cruzam em 16 batidas; a anterior para sozinha.
  - **Play na batida** (como o Smart Play; CONFIG → CONTROLADORA, desligado de fábrica): dar PLAY com
    o outro deck tocando já entra na batida dele.
  - **Tom (key)**: subir/descer até ±6 semitons sem mudar a velocidade e **casar o tom** com o
    outro deck (tocar no TOM do cartão do deck, ou botões pelo MIDI learn).
  - **"♪ Combina com o deck N"** no topo das pastas do browser: músicas com BPM até 6% (ou
    metade/dobro) e tom compatível com o que toca no ar, as do mesmo tom primeiro.
  - **Playlists**: as .m3u/.m3u8 do pendrive (exportadas do VirtualDJ ou do rekordbox, inclusive
    feitas no Windows) aparecem nas pastas; a fila vira playlist salva no aparelho
    (CONFIG → TELA, Ctrl+P ou botão ensinado), em "☰ Playlists".
  - **Filtro do browser** (tocar em FILTRO no canto, tecla F ou botão ensinado): todas, não
    tocadas, BPM do que toca, tom que combina.
  - **Efeitos novos**: ROLL, CRUSH, BRAKE e BACKSPIN (WAVs para ouvir com `dj-os --demo-efeitos`).

### Corrigido
- **Som sem prioridade (chiado e estalos, principalmente com música longa):** o som disputava o
  processador de igual para igual com a tela, a análise e o pendrive. Quanto mais longa a
  música, mais tempo abrindo e analisando, e mais estalo. Eram dois defeitos:
  - no aparelho, o PipeWire pedia prioridade de tempo real e o sistema negava (faltava a
    permissão; o registro mostrava "RTKit error"). Agora o usuário do aparelho tem a permissão
    (`/etc/security/limits.d/30-dj-os.conf`);
  - o DJ OS abria o PipeWire com a configuração comum, que no PipeWire 1.0 não pede prioridade
    para a thread do som (no aparelho e no PC). Agora abre com a de tempo real
    (`client-rt.conf`, quando existe).
  - O registro anota ao abrir a saída: "som: prioridade de tempo real ligada (nível N)" ou
    "SEM prioridade de tempo real".
- **"NO AR" e a trava do LOAD consideram o crossfader:** com o crossfader fechado para o outro
  lado o deck não está no ar (mesmo com o fader do canal em cima) e o LOAD funciona.
- **Voltar para um firmware anterior não perde mais os cues:** a memória de cada música continua
  no formato que os firmwares antigos leem.
- **BPM errado no funk:** o tamborzão (3+3+2) enganava o detector, que pegava 1,5 batida como se
  fosse uma e dava 2/3 do BPM (130 → 86,66), deixando a grade apertada numa música e espaçada na
  outra. O detector agora confere se a batida se repete a cada 2 e 4 batidas. Medido em 85
  eletrofunks: de 40 para 65 em 130 BPM e nenhum mais em 86,66; em 103 músicas de house nada
  mudou (fora 2 mixagens com vários BPMs).
  - As músicas já analisadas medem o BPM de novo sozinhas no LOAD, e a grade guardada só é
    trocada quando é esse conserto (o BPM guardado era 2/3 ou 1,5× o novo). Qualquer outra grade
    (acertada à mão, do rekordbox) fica como estava; cues, hot cues e loops ficam sempre.
  - No ajuste da grade (tocar no BPM): botões **×1,5** (86,66 vira 130) e **×2/3**, além do ÷2 e ×2.
- **Estalos e estática no som depois de um tempo** (fone e master), principalmente ao carregar um
  set longo (50 min ocupam ~1 GB e levam ~30 s para analisar num notebook antigo) ou com o HD
  inteiro sendo lido para a pesquisa:
  - abrir e analisar a música no LOAD e ler o HD agora rodam com prioridade baixa (antes
    disputavam o processador com o som);
  - mais folga no som: 512 amostras (10,7 ms) em vez de 256 (5,3 ms). Em CONFIG → ÁUDIO →
    "Folga do som": 256, 512 ou 1024 (computador fraco); a saída reabre sozinha ao trocar.
- O registro do aparelho agora anota, a cada minuto, se o som atrasou (e quanto), com a
  temperatura e a velocidade do processador: dá para ver se foi calor.

### Novo
- Preparar: analisa mais músicas ao mesmo tempo (núcleos menos 2, até 10, conforme a memória
  livre); antes eram no máximo 4. Num i5 de 12 núcleos, 12 mil músicas em ~25-30 min.

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
