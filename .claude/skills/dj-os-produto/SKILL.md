---
name: dj-os-produto
description: Jeito de pensar do DJ OS como produto de uma empresa de equipamentos de DJ, gratuito e open source. Use ao planejar ou mexer em qualquer recurso, desempenho, biblioteca/análise de músicas, distribuição (ISO, firmware, app do PC, Windows/Linux), README ou lançamento do DJ OS.
---

# DJ OS como produto

Pense como a equipe de uma empresa que faz equipamento de DJ (estilo Pioneer/AlphaTheta), só que
o produto é **gratuito e open source**. O dono do projeto é DJ, não programador: quem programa e
decide o "como" é você; ele decide o "o quê" e testa no aparelho de verdade.

## Dois programas, um formato

1. **DJ OS (aparelho)**: vai dentro da ISO, roda na máquina da cabine. Só toca. Não pode pesar,
   travar nem esquentar a máquina.
2. **DJ OS Preparar (computador de casa)**: igual ao rekordbox. A pessoa escolhe o pendrive e ele
   analisa tudo (BPM, grade, tom, volume, waveform, cues) e grava em `DJOS/analise/` dentro do
   próprio pendrive. Windows e Linux.

O formato do pendrive é o contrato entre os dois (`src/store.rs`: arquivos `.dxj`, chave nome +
tamanho). Mudou o formato? Os dois lados continuam lendo o antigo.

No aparelho, análise só no LOAD (uma música por vez) ou quando a pessoa pede ("Analisar todas as
músicas"), e nunca enquanto um deck toca. Nada de trabalho pesado em segundo plano.

## Roda em máquina fraca

A referência é a máquina de teste do dono: notebook de 2011, i5-2430M (2 núcleos), Intel HD 3000,
sem bateria, refrigeração ruim (passou de 95 °C e desligou). Se roda bem nela, roda em qualquer uma.
Pense também em Raspberry Pi no futuro.

- **Meça antes e depois** (CPU por thread com o modo `DJOS_DEMO=1`; ver docs/desenvolvimento).
  Tocando dois decks, o alvo é poucos % de um núcleo num PC moderno.
- Trabalho pesado (decodificar, analisar, copiar) com prioridade baixa e longe da thread de áudio
  e da tela. A thread de áudio nunca espera trava (só `try_lock`/filas).
- Calor e energia fazem parte do produto: controle de calor (`dj-os-energia`), sem turbo, registros
  de temperatura que sobrevivem a corte de energia.
- Travas (mutex) sempre na mesma ordem; o vigia de deadlock registra e reabre o programa.

## Confiável como equipamento de cabine

- Nada de perder o set: se o programa cair, volta com as músicas nos decks; atualização de
  firmware A/B com volta automática.
- Todo problema deixa rastro que o dono consegue trazer num pendrive (registros no Ventoy e
  "copiar registros"). Ele não sabe ler log: você lê e explica em português simples.

## Distribuição para qualquer pessoa

- Nada de linha de comando para o usuário final: ISO para gravar no pendrive/Ventoy, firmware
  `.djos` pelo menu, app do PC em `.exe` (Windows) e AppImage (Linux), dois cliques e abre.
- Tudo publicado pelo GitHub Releases (`.github/workflows/`), montado automaticamente ao publicar
  uma versão.
- README de projeto open source caprichado: o que é, prints da tela (`docs/imagens/`), como
  instalar em passos curtos, requisitos, licença. Tem que parecer produto, não experimento.
- Sem marcas de terceiros na tela (Pioneer é referência de layout e de hardware suportado, só).

## Ao propor algo

Explique para o dono em linguagem de DJ, sem jargão: o que muda na cabine, o que foi medido e o
que ainda falta testar na máquina dele.
