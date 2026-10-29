# Histórico de versões

O formato segue [Keep a Changelog](https://keepachangelog.com/pt-BR/1.1.0/) e a versão segue
*maior.menor.correção*. Cada montagem leva também a data e a hora (ex.: `0.1.0-20261029.1245`),
para diferenciar firmwares de teste da mesma versão.

## [Não lançado]

### Adicionado
- Configurações em abas (estilo XDJ), com resumo de cada aba e navegação pelo BROWSE.
- Tela de inicialização com a logo do DJ OS; menu de boot escondido no aparelho instalado (Esc mostra).
- Botão power desliga com segurança; fechar a tampa ou teclas de suspender não fazem nada.
- Limitador no master e no fone: o som nunca passa de -1 dBFS (aviso "LIM" na barra de cima).

### Corrigido
- Eco na caixa Bluetooth (o master ia duas vezes para ela).
- Scratch: o backspin segue o prato solto até ele parar.
- Celular por Bluetooth: pareamento sem pedir código e nome "DJ OS"; visibilidade com prazo.

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
