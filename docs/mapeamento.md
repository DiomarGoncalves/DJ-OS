# Como mapear uma controladora no DJ OS

Cada controladora é um arquivo de texto `.djosmap`: uma linha por botão, knob, fader ou LED.
Não precisa programar nem recompilar nada.

- Os mapeamentos que já vêm com o DJ OS estão na pasta [`mapeamentos/`](../mapeamentos) do projeto.
- Os seus ficam em **`~/DJOS/mapeamentos/`** (no Windows, `C:\Users\<você>\DJOS\mapeamentos\`).
  Um arquivo seu com o mesmo `nome` de um que vem com o DJ OS substitui o dele.
- O DJ OS relê os arquivos sempre que não há controladora conectada: salve o arquivo, tire e
  ligue o USB da controladora de novo e pronto.
- Se uma linha estiver errada, o arquivo fica de fora e o erro (com o número da linha) vai para o
  registro do DJ OS. Para ver na hora, rode o DJ OS pelo terminal.

A Pioneer DDJ-REV1 tem o mapeamento escrito no próprio programa (testado botão por botão); as
outras controladoras usam estes arquivos.

## Descobrir o que a controladora manda

Em **CONFIG → CONTROLADORA → Registrar os botões (diagnóstico, 5 min)**, cada botão apertado ou
knob girado aparece no registro como três números em hexadecimal, por exemplo:

```text
[midi] 90 0B 7F      ← nota (9x), canal 1 (x = 0), número 0x0B, apertado (7F)
[midi] 80 0B 00      ← nota solta
[midi] B0 13 40      ← CC (Bx), canal 1, número 0x13, valor 0x40
[midi] E1 00 40      ← pitch bend (Ex), canal 2
```

O primeiro número diz o tipo e o canal: `9x`/`8x` = nota, `Bx` = CC, `Ex` = pitch bend. O
canal é `x + 1` (`90` = canal 1, `96` = canal 7, `9F` = canal 16). A documentação oficial de
cada controladora (a "MIDI Message List" da Pioneer, por exemplo) traz a mesma tabela.

Também dá para ensinar botão por botão sem editar o arquivo: **CONFIG → CONTROLADORA → Ensinar
um botão (MIDI learn)**, escolha a função e aperte o botão na controladora.

## O arquivo

```text
# Comentários começam com #.
nome = Pioneer DDJ-400
porta = DDJ-400            # parte do nome da porta MIDI (pode ter várias linhas "porta =")
situacao = em teste        # "testado" quando alguém conferiu com o aparelho
fonte = MIDI Message List da Pioneer DJ (DDJ-400)

# tipo  canal  número  ação          opções
nota    1      0x0B    play          deck=1
nota    1      0x0C    cue           deck=1
cc      1      0x13    volume        deck=1 lsb=0x33
cc      1      0x22    jog_scratch   deck=1
nota    7      0x3F    shift
led nota 1     0x0B    tocando       deck=1
```

- **tipo**: `nota`, `cc` ou `pb` (pitch bend, não tem número: `pb 2 tempo deck=2`).
- **canal**: de 1 a 16.
- **número**: de 0 a 127, em decimal (`11`) ou hexadecimal (`0x0B`).
- **ação**: o que o controle faz (lista abaixo).
- **opções** (`nome=valor`, em qualquer ordem):

| opção | para quê |
|---|---|
| `deck=1` … `deck=4` | o deck do controle (obrigatório nas ações de deck) |
| `n=1` … `n=8` | número do pad ou do hot cue |
| `b=4`, `b=-1`, `b=1/2` | batidas (`loop_auto`, `beatjump`) |
| `lsb=0x33` | CC com a parte fina do valor (14 bits): fader e knobs mais precisos |
| `shift` | a linha só vale com SHIFT segurado (para controladoras que mandam a mesma nota com e sem SHIFT) |
| `invertido` | o valor ao contrário (fader de pitch de cabeça para baixo) |
| `valor=botao` / `absoluto` / `relativo` / `relativo64` | como ler o valor (cada ação já tem o certo; troque só se precisar) |
| `modo=hotcue` … | modo dos pads (`pad_modo`) |

`valor`:
- `botao`: nota ligada ou valor maior que 0 = apertado; nota solta ou 0 = solto.
- `absoluto`: posição de 0 a 127 (de 0 a 16383 com `lsb=`).
- `relativo`: encoder que manda 1, 2, 3… para um lado e 127, 126, 125… para o outro (BROWSE da maioria).
- `relativo64`: encoder com o meio em 64 (65, 66… para um lado, 63, 62… para o outro): jogs da Pioneer e da maioria das marcas.

## Ações

### De deck (precisam de `deck=`)

| ação | o que faz |
|---|---|
| `play` | play / pausa |
| `cue` | CUE estilo CDJ (segurar toca a partir do cue) |
| `sync` | sincroniza com o outro deck |
| `pitch_zero` | pitch volta a 0 |
| `stutter` | volta ao cue e continua tocando |
| `inicio` | volta ao começo da música |
| `load` | carrega a música selecionada no browser (numa pasta, abre a pasta) |
| `loop` | liga/desliga o loop automático |
| `loop_sair` | reloop / sai do loop |
| `loop_metade`, `loop_dobro` | loop pela metade / em dobro |
| `loop_in`, `loop_out` | marca o começo / o fim do loop |
| `loop_auto` | loop de `b=` batidas |
| `beatjump` | pula `b=` batidas (negativo volta) |
| `pfl` | CUE do fone liga/desliga |
| `vinil` | modo vinil (jog faz scratch) liga/desliga |
| `keylock`, `slip` | liga/desliga |
| `faixa_pitch` | troca a faixa do pitch (8 → 16 → 50%) |
| `volume` | fader do canal |
| `tempo` | fader de pitch |
| `trim` | ganho do canal |
| `grave`, `medio`, `agudo` | EQ |
| `filtro` | filtro do canal (meio = desligado) |
| `jog_toque` | encostar a mão no prato |
| `jog_scratch` | prato girado com a mão em cima (scratch) |
| `jog_bend` | lateral do jog (ajuste fino da velocidade) |
| `jog_busca` | jog com SHIFT (anda rápido pela música) |
| `hotcue` | hot cue `n=` (vazio marca, marcado toca) |
| `apagar_hotcue` | apaga o hot cue `n=` |
| `pad` | pad `n=`: faz o que o modo atual dos pads mandar (hot cue, loop, roll, beat jump, sampler…) |
| `pad_modo` | troca o modo dos pads: `modo=hotcue`, `loop`, `roll`, `beatjump`, `sampler`, `tracking`, `trans`, `scratch` |
| `fx_liga` | liga/desliga o efeito do deck |
| `fx_nivel` | força do efeito |
| `fx_batida_menos`, `fx_batida_mais` | divisão de batida do efeito |
| `fx_proximo` | troca o efeito |
| `deck_ativo` | DECK SELECT: este deck passa a ser o do seu lado (controladoras de 4 decks) |

### Gerais

| ação | o que faz |
|---|---|
| `shift` | o SHIFT |
| `crossfader` | crossfader |
| `master` | volume do master |
| `fone_volume` | volume do fone |
| `fone_mix` | mistura CUE/master no fone |
| `browse_gira` | BROWSE girado (anda na lista; segurando o BROWSE, zoom da waveform) |
| `browse_aperta` | BROWSE apertado (pastas/músicas; segurar abre a pesquisa) |
| `browser` | abre/fecha o browser em tela cheia |
| `config` | abre/fecha as configurações |

## LEDs

```text
led nota 1 0x0B tocando deck=1
led nota 8 0x00 hotcue deck=1 n=1 ligado=0x7F desligado=0x01
```

`led` + tipo + canal + número + estado + opções. O DJ OS manda `ligado=` (padrão 127) quando o
estado está valendo e `desligado=` (padrão 0) quando não. Algumas controladoras usam o valor para
a cor ou para meia-luz: veja a documentação dela.

| estado | acende quando |
|---|---|
| `tocando` | o deck está tocando |
| `cue` | o deck está parado no cue (ou tocando pelo CUE segurado) |
| `sync`, `pfl`, `loop`, `vinil`, `keylock`, `slip` | aquilo está ligado |
| `hotcue` | o hot cue `n=` está marcado |
| `pad` | o pad `n=` tem algo (no modo hot cue: o hot cue marcado) |
| `pad_modo` | os pads estão no modo `modo=` |
| `fx_ligado` | o efeito do deck está ligado |
| `shift` | o SHIFT está segurado (não precisa de `deck=`) |

## Mandar o seu mapeamento para o DJ OS

Ficou bom? Abra uma issue ou um pull request no GitHub com o arquivo em `mapeamentos/` e diga se
conferiu com o aparelho (`situacao = testado`). Assim ele passa a vir com o DJ OS para todo mundo.
