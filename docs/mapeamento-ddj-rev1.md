# Mapeamento da DDJ-REV1 no DJ OS

Botão por botão. Baseado no mapeamento personalizado do Diomar no VirtualDJ e nas mensagens da lista
oficial da Pioneer ("DDJ-REV1 List of MIDI Message", disponível no site da Pioneer DJ; não incluída aqui).
Cada linha tem teste automático em `src/midi/tests.rs`.

"n" é o deck: 0 = deck 1, 1 = deck 2, 2 = deck 3, 3 = deck 4. "Lado" é a metade da controladora:
esquerda (deck 1 ou 3) e direita (deck 2 ou 4).

## Deck (canal MIDI do deck: status 9n / Bn)

| Controle | MIDI | Função |
|---|---|---|
| PLAY/PAUSE | 9n 0B | Toca / pausa. Apertado enquanto segura CUE ou hotcue: continua tocando |
| SHIFT + PLAY | 9n 47 | Volta ao cue e toca (stutter) |
| CUE | 9n 0C | Estilo CDJ: tocando, volta ao cue e para; parado no cue, toca enquanto segura; parado fora do cue, marca o cue |
| SHIFT + CUE | 9n 48 | Volta ao início da música |
| Jog (prato, vinil ligado) | Bn 22 | Scratch enquanto toca no prato |
| Jog (prato, vinil desligado) / lateral | Bn 23 / Bn 21 | Pitch bend (tocando) ou anda devagar pela música (parado) |
| SHIFT + jog | Bn 1F / Bn 26 | Busca rápida |
| Toque no jog | 9n 36 | Segura o prato (scratch). No modo Tracking, soltar volta ao cue |
| DECK SELECT | 92/93 72 | Troca entre deck 1/3 (esquerda) e 2/4 (direita) |
| SHIFT + DECK SELECT | 9n 17 | Vinil (scratch) / CDJ (bend) |
| DECK SELECT segurado | 9n 60 | Faixa de pitch: 8 → 16 → 50% |
| Tempo | Bn 00/20 | Pitch (14 bits). Depois de SYNC ou reset, só volta a valer quando o fader passa pelo valor atual |
| AUTO LOOP | 9n 14 | Liga/desliga o loop no tamanho atual (começa na batida) |
| SHIFT + AUTO LOOP | 9n 50 | Reloop / sair do loop |
| 1/2X / 2X | 9n 12 / 9n 13 | Loop pela metade / em dobro |
| SHIFT + 1/2X / 2X | 9n 10 / 9n 11 | Loop in / loop out |
| SYNC | 9n 58 | Iguala BPM e fase com o outro deck tocando |
| SHIFT + SYNC | 9n 5C | Pitch volta a 0 |

## Mixer

| Controle | MIDI | Função |
|---|---|---|
| GAIN (TRIM) | Bn 04/24 | Ganho do canal (até +9 dB) |
| EQ HI / MID / LOW | Bn 07, 0B, 0F (+20) | Corte total no mínimo, 0 dB no centro, +6 dB no máximo |
| Fader do canal | Bn 13/33 | Volume do canal |
| SHIFT + fader / crossfader | 9n 66 / 51 / 52 | Fader start: play / sync / volta ao cue |
| FILTER CH1-4 | B6 17-1A (+20) | Filtro do canal: esquerda abafa (passa-baixas), direita afina (passa-altas) |
| Crossfader | B6 1F/3F | Os dois lados cheios no meio |
| MASTER LEVEL / HEADPHONES LEVEL | B6 08/28, 0D/2D | Volume do master / do fone |
| CUE do fone (PFL) | 9n 54 | Liga/desliga o canal no fone. Com MASTER CUE ligado: fone 50/50 (ou 100% master se nenhum PFL) |
| MASTER CUE | 96 63 | Liga: master no fone (100%, ou 50% se houver PFL). Desliga: fone só com os PFLs |

## Efeitos (um por deck; FX1 = lado esquerdo, canal 5; FX2 = lado direito, canal 6)

| Controle | MIDI | Função |
|---|---|---|
| Alavanca | 94/95 50, 66, 47-49 | Liga (empurrada) / desliga (soltando) o efeito do deck do lado |
| FX 1 (com ou sem SHIFT) | 94/95 70, 63 | Mostra o próximo efeito na lista; parando de clicar 1 s, ele entra no deck |
| FX 2 / FX 3 (com ou sem SHIFT) | 94/95 71, 06 / 72, 07 | Divisão de batida menor / maior: 1/16, 1/8, 1/4, 1/2, 3/4, 1, 1 1/2, 2, 4, 8, 16 |
| Knob central LEVEL/DEPTH | B4/B5 02, 04, 06 (+20) | Força do efeito (quanto do som do efeito entra) |
| BROWSE girado segurando um FX | B4 50-52 / B5 53-55 | Anda pela lista de efeitos nos dois sentidos |

Efeitos: ECHO, PING PONG, REVERB, FLANGER, PHASER, WAHWAH, NOISE, CUT. Echo e reverb continuam
soando ao desligar a alavanca (a cauda some sozinha). O LED do FX 1 fica aceso (pisca com o efeito
ligado): é o que faz o knob central mandar a força. **Conferir com a REV1 na mão.**

## Browser e pesquisa (canal 7)

| Controle | MIDI | Função |
|---|---|---|
| BROWSE girado | B6 40 | Anda pela lista (abre a lista em tela cheia se estiver na tela principal) |
| SHIFT + BROWSE girado | B6 64 | Anda pela lista "Preparar" |
| BROWSE, toque curto | 96 41 (ao soltar) | Entra nas músicas da pasta / volta para as pastas |
| BROWSE segurado 0,5 s | 96 41 | Abre a pesquisa com teclado na tela |
| SHIFT + BROWSE | 96 42 | Abre/fecha a lista em tela cheia |
| LOAD esquerdo / direito | 96 46 (deck 1), 47 (deck 2), 48 (deck 3), 49 (deck 4) | Carrega a música e volta para a tela principal. Numa pasta: abre/fecha as subpastas. Dois toques rápidos: clona o deck do outro lado |
| SHIFT + LOAD | 96 58, 59, 60, 61 | Põe a música na lista "Preparar" |

Com o teclado da pesquisa aberto: girar o BROWSE escolhe a tecla, apertar digita, e os pads viram
1 ↑, 2 ↓, 3 ←, 4 →, 5 confirma, 6 apaga, 7 espaço, 8 fecha. ENTER mostra os resultados na lista.

## Pads (canais 8-15: 97 deck 1, 98 deck 1 + SHIFT, 99 deck 2, 9A, 9B deck 3, 9C, 9D deck 4, 9E)

| Botão de modo | MIDI (deck) | Modo | Pads 1-8 | Com SHIFT |
|---|---|---|---|---|
| HOT CUE | 9n 1B | Hot cue | Marca / pula (parado: toca enquanto segura) | Apaga o hotcue |
| SHIFT + HOT CUE | 9n 69 | Beat jump | ←1 1→ ←2 2→ ←4 4→ ←8 8→ batidas | 4 vezes mais |
| AUTO LOOP | 9n 1E | Auto loop | 1/4, 1/2, 1, 2, 4, 8, 16, 32 batidas | Igual |
| SHIFT + AUTO LOOP | 9n 6B | Roll | 1/32, 1/16, 1/8, 1/4, 1/2, 1, 2, 4 enquanto segura; ao soltar volta onde estaria | Igual |
| TRACKING | 9n 20 | Tracking | Toca do cue em 0, +2, +4, +5, +7, +9, +11, +12 semitons; soltar o jog volta ao cue | Uma oitava abaixo |
| SHIFT + TRACKING | 9n 6D | Trans | Corta o som no ritmo de 1, 1/2, 1/4, 1/8, 1/16, 2/3, 1/3, 1/6 batida enquanto segura | Igual |
| SAMPLER | 9n 22 | Sampler | Toca o sample 1-8 (`~/DJOS/samples`) | Para o sample |
| SHIFT + SAMPLER | 9n 6F | Scratch bank | Põe o sample 1-8 (`~/DJOS/scratch`) no deck | Devolve a música original |

Os pads da tela fazem exatamente o mesmo (mesma função no código).

## Configurações (SHIFT + MASTER CUE abre e fecha)

Abas à esquerda (Áudio, Caixa Bluetooth, Celular, Wi-Fi, Controladora, Tela, Sistema), itens à
direita.

| Ação | Função |
|---|---|
| BROWSE girado, nas abas | Troca de aba |
| BROWSE apertado, nas abas | Entra na aba |
| BROWSE girado, nos itens | Anda pelos itens; girar para cima no primeiro item volta às abas |
| BROWSE apertado, nos itens | Aciona o item (Reiniciar, Desligar e instalar firmware pedem um segundo toque) |
| Toque / clique | Na aba abre a aba; no item aciona; "X FECHAR" fecha |
| Teclado | F10 abre/fecha · ↑↓ andam · ← abas · → itens · Enter aciona · Esc fecha |

## Na tela (toque ou mouse)

| Onde | Função |
|---|---|
| BPM do deck | Abre o ajuste da grade de batidas (BPM, TAP, deslocar, "BATIDA AQUI") |
| Waveform | Troca o zoom |
| GRAVAR (barra de cima) | Grava / para a gravação do set (também Ctrl+R) |
| Pads | Mesmas funções dos pads da REV1 no modo atual |

## Teclado do computador (para testar sem a REV1)

B browser · Ctrl+F pesquisa · Ctrl+R grava o set · setas navegam · Tab pastas/músicas · Enter abre pasta · 1-4 carregam ·
Z/M play · X/N cue · S/K sync · F1/F5 alavanca · F2-F3/F6-F7 batida · F4/F8 troca efeito · +/- zoom ·
setas esq/dir crossfader.
