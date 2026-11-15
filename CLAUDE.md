# Regras do projeto DJ OS

## Mudou o som, testa o som (regra do Diomar, 2026-11-15)

Os pocos do show de 13/11 vieram de uma atualização do motor que deixou o deck parado 30× mais
lento no notebook velho (números denormais). Antes não acontecia. Por isso, **toda mudança no
motor de áudio** (`src/engine/`, `src/pw_out.rs`, efeitos, keylock, leitura da música em
`analise/src/track.rs`) só vai para firmware depois de:

1. `cargo test --release` passar inteiro, incluindo `som_aguenta_o_show` (mede cada situação do
   show: tocando, com efeitos, keylock, parado com a cauda dos efeitos). Se a mudança cria uma
   situação nova (efeito novo, modo novo), ela entra nesse teste.
2. O mesmo teste imitando o notebook velho:
   `ferramentas/lento.sh cargo test --release -p dj-os som_aguenta_o_show -- --nocapture`.
3. Comparar os tempos com os de antes da mudança e anotar no commit. Se piorou, explicar por quê.
4. Mudança que altera o som em si: WAVs de antes e depois para ele ouvir.

Medidas de referência (2026-11-15, PC de desenvolvimento / imitando o notebook velho), por bloco
de 10,7 ms: tocando 0,04 / 0,20 ms; com EQ, filtro e efeito 0,10 / 0,35 ms; keylock com pitch
0,67 / 1,48 ms; parado 0,05 / 0,15 ms.
