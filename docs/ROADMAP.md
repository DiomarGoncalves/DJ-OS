# Ideias para depois

Prioridade agora: o sistema 100% redondo com a DDJ-REV1. Estas ficam anotadas para quando o
sistema estiver consolidado.

## Outras controladoras (DDJ-400, DDJ-FLX4, …)
- ✅ Catálogo de controladoras (`MODELS` em `src/midi/mod.rs`): cada uma é reconhecida pelo nome
  da porta MIDI e tem o seu mapeamento; as desconhecidas aparecem como "ainda não suportada".
  Guia em [controladoras.md](controladoras.md).
- Falta: os mapeamentos em si (DDJ-400, FLX4…), com a lista MIDI de cada uma e alguém para testar.
- Depois: um assistente de "aprender" (aperte o botão na controladora, escolha a ação na tela)
  para mapear controladoras novas sem programar.

## Caixa Bluetooth e celular ao mesmo tempo
- Hoje, conectar a caixa derruba o celular (e vice-versa). Investigar com o registro do BlueZ no
  momento da conexão. Alternativa garantida: um segundo adaptador Bluetooth USB (um recebe do
  celular, o outro manda para a caixa).

## Outras plataformas
- **Raspberry Pi / ARM (aarch64):** o programa em Rust compila para ARM; falta montar a imagem
  (raiz Ubuntu/Debian arm64 e o boot próprio do Raspberry, que não usa GRUB) e medir o desempenho
  do áudio e da tela.
- **Android:** ideia para o futuro (celular velho como aparelho).

## Recursos de DJ (sugeridos)
- Tom da música (key) com cores para mixagem harmônica.
- Gravar o set no pendrive; histórico do que tocou.
- Ajuste da grade de batida; ganho automático; limitador no master.
