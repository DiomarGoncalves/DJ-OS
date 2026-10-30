# Histórico de versões

O formato segue [Keep a Changelog](https://keepachangelog.com/pt-BR/1.1.0/) e a versão segue
*maior.menor.correção*. Cada montagem leva também a data e a hora (ex.: `0.1.0-20261029.1245`),
para diferenciar firmwares de teste da mesma versão.

## [Não lançado]

### Novo
- Aba **MIXER** nas configurações (inspirada nos mixers de clube):
  - modo do EQ: isolador (corta tudo no mínimo) ou EQ (até -26 dB);
  - curva do fader de canal: lenta, normal ou rápida (corte);
  - curva do crossfader (saiu da aba Controladora);
  - lado de cada deck no crossfader: A, B ou THRU (fora do crossfader);
  - fone em estéreo ou mono dividido (CUE no ouvido esquerdo, master no direito).

### Corrigido
- Som baixo na REV1 mesmo com tudo no máximo: o sistema agora deixa a saída da REV1 em 100%
  (no PipeWire e na placa) e registra como estava.
- Botão MASTER: o meio agora é 0 dB (antes -6 dB), e a metade de baixo também tem volume.
- Scratch e backspin dentro do loop: para trás, o som volta pelo fim do loop em vez de sair dele.
- Mão no prato: o som freia liso e fica mudo com a mão parada (antes chiava e deixava um zumbido);
  o scratch segue a mão sem o "serrilhado" digital e para exatamente onde a mão parou.

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
