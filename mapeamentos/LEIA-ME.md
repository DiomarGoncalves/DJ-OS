# Mapeamentos de controladora

Cada arquivo `.djosmap` desta pasta vai junto dentro do DJ OS (o programa é montado com todos).
O formato está em [docs/mapeamento.md](../docs/mapeamento.md). Para usar um mapeamento seu sem
recompilar, ponha o arquivo em `~/DJOS/mapeamentos/`.

`situacao = em teste`: montado pela documentação, ainda sem alguém conferir com o aparelho.
Testou e funcionou? Mude para `situacao = testado` e mande para o projeto.

## De onde vieram os endereços

Todos começam como `situacao = em teste`: foram montados pela documentação, sem a controladora na
mão. Cada arquivo diz a fonte na linha `fonte =`.

- **Pioneer DDJ-400, FLX4, SB3**: mapeamentos do Mixxx (GPL-2.0 ou posterior).
- **Pioneer DDJ-SR2 e DDJ-1000**: propostas de mapeamento ainda não incorporadas ao Mixxx (a do
  DDJ-1000 feita a partir da lista MIDI oficial).
- **Pioneer DDJ-FLX6**: mapeamento da comunidade adaptado do FLX4 do Mixxx; o menos conferido.
- **Pioneer CDJ-2000NXS2, CDJ-3000, CDJ-900NXS, XDJ-RX2, XDJ-RX3, XDJ-XZ**: listas MIDI oficiais
  da Pioneer DJ (manuais e "MIDI Message List").
- **Numark, Hercules, Denon, Reloop**: mapeamentos do Mixxx (GPL-2.0 ou posterior).

Obrigado aos autores dos mapeamentos do Mixxx e da comunidade.
