# Como fazer uma skin para o DJ OS

Uma skin é um arquivo de texto `.djosskin` com as cores e o layout da tela. Não precisa
programar nem recompilar.

- As skins que vêm com o DJ OS estão na pasta [`skins/`](../skins) do projeto: **Preta** (a de
  fábrica, com todas as opções escritas: um bom ponto de partida), **Clara**, **Neon** e
  **Waveform grande**.
- As suas ficam em **`~/DJOS/skins/`** (no Windows, `C:\Users\<você>\DJOS\skins\`). Uma skin sua
  com o mesmo `nome` de uma que vem com o DJ OS substitui a dele.
- Para escolher: **CONFIG → TELA → Skin** (cada toque passa para a próxima). Editou o arquivo?
  Toque em Skin de novo até voltar nela: o DJ OS relê os arquivos.
- Se uma linha estiver errada, a skin fica de fora e o erro (com o número da linha) vai para o
  registro do DJ OS.

## O arquivo

```text
# Linhas que começam com # são comentários.
nome = Minha skin

fundo = #000000
texto = #ECECEC
destaque = #1E8CFF

painel_efeitos = 0      # esconde o painel de efeitos
```

Cada linha é `chave = valor`. **Só escreva o que quiser mudar**: o resto fica como na skin de
fábrica. As cores são no formato `#RRGGBB` (o mesmo dos editores de imagem e dos sites de
paleta de cores).

## Cores

| chave | onde aparece |
|---|---|
| `fundo` | fundo da tela |
| `painel` | painéis (cartão do deck, efeitos, decks embaixo) |
| `painel2` | cabeçalhos dos painéis, botões |
| `linha` | linhas e bordas |
| `texto` | textos |
| `apagado` | textos secundários (rótulos, coisas desligadas) |
| `destaque` | destaque (títulos, coisa escolhida) |
| `laranja` | avisos e o loop |
| `vermelho` | "NO AR", gravação, alertas |
| `verde` | sync, tom que combina, música carregada |
| `selecao` | linha selecionada no browser |
| `selecao_fraca` | linha selecionada do lado que não está com o BROWSE |
| `deck1` … `deck4` | cor de cada deck (faixa lateral, número) |
| `grave`, `medio`, `agudo` | as três camadas da waveform (com a paleta AZUL/LARANJA em CONFIG → TELA) |
| `cue_a` … `cue_h` | cor de cada hot cue |

## Layout da tela principal

| chave | o que faz | de fábrica |
|---|---|---|
| `painel_deck` | largura (em pontos) do cartão de cada deck à esquerda das waveforms; `0` esconde | `176` |
| `painel_efeitos` | largura do painel de efeitos à direita; `0` esconde | `236` |
| `hotcues` | faixa dos hot cues embaixo das waveforms (`sim` / `nao`) | `sim` |
| `decks_embaixo` | painéis dos decks embaixo (tempo, BPM, música inteira) (`sim` / `nao`) | `sim` |

O que sobra de espaço vai para as waveforms. Os 2 ou 4 decks embaixo continuam na opção
CONFIG → TELA → "Decks embaixo da tela".

## Dicas

- Cabine com muita luz: fundo claro e texto escuro (veja a skin **Clara**). Com fundo claro,
  troque também a cor `agudo` da waveform por uma escura, senão o agudo some.
- Contraste: texto e fundo bem diferentes; o DJ olha a tela de longe e com a luz da pista.
- Cores dos hot cues: as de fábrica são as mesmas dos pads da controladora; se mudar, mude as oito.

## Mandar a sua skin para o DJ OS

Ficou bonita? Abra uma issue ou um pull request no GitHub com o arquivo em `skins/` (e um print).
Assim ela passa a vir com o DJ OS para todo mundo.
