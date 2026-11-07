# Controladoras

O DJ OS reconhece a controladora pelo nome da porta MIDI e usa o mapeamento dela. Uma
controladora que ainda não tem mapeamento aparece na barra de cima como
"<nome>: ainda não suportada".

- **Pioneer DDJ-REV1:** mapeamento escrito no programa (`src/midi/rev1.rs`), testado botão por
  botão; guia em [mapeamento-ddj-rev1.md](mapeamento-ddj-rev1.md).
- **As outras:** cada uma é um arquivo `.djosmap` em [`mapeamentos/`](../mapeamentos) (a lista e as
  fontes estão no [LEIA-ME](../mapeamentos/LEIA-ME.md) de lá e no README).

## Como acrescentar uma controladora

Não precisa programar: escreva um arquivo `.djosmap` seguindo [mapeamento.md](mapeamento.md)
(uma linha por botão, knob, fader e LED, com os números da "MIDI message list" do fabricante),
ponha em `~/DJOS/mapeamentos/` e ligue a controladora. Para ajustar só alguns botões, use o
**MIDI learn** (CONFIG → CONTROLADORA → Ensinar botões): ele vale por cima do mapeamento.

Ficou bom? Mande o arquivo para `mapeamentos/` do projeto (pull request): ele passa a vir dentro
do DJ OS para todo mundo.

## Placa de som da controladora

Controladoras com placa de som de 4 canais (master + fone) são reconhecidas pelo nome
(`is_rev1`/`is_controladora` em `src/pw_out.rs`): master nas saídas 1-2, fone nas 3-4. Se a sua não
for reconhecida, escolha em CONFIG → ÁUDIO → Saída do som (4 canais ou mais: master 1-2, fone 3-4).

## Controladora com comportamento especial

Se a controladora precisa de algo que o arquivo não consegue dizer (mensagens SysEx na conexão,
LED com a cor no lugar do número, encoders esquisitos), dá para escrever o mapeamento em Rust como
o da REV1: um arquivo em `src/midi/` com uma estrutura que implementa `super::Controller`
(`handle` traduz cada mensagem em comandos de `src/shared.rs`; `leds` diz o que acender) e uma
entrada a mais em `catalogo()` (`src/midi/mod.rs`). Testes no jeito de `src/midi/tests.rs`.
