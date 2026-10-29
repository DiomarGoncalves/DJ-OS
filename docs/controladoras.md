# Controladoras

O DJ OS reconhece a controladora pelo nome da porta MIDI e usa o mapeamento dela. Uma
controladora que ainda não tem mapeamento aparece na barra de cima como
"<nome>: ainda não suportada".

| Controladora | Situação | Mapeamento |
|---|---|---|
| Pioneer DDJ-REV1 | Suportada (master 1-2, fone 3-4, LEDs) | `src/midi/rev1.rs`, guia em [mapeamento-ddj-rev1.md](mapeamento-ddj-rev1.md) |

## Como acrescentar uma controladora

1. **Lista MIDI do fabricante:** quase toda controladora tem um PDF "MIDI message list" no site do
   fabricante, com o código de cada botão, knob e LED. (Não inclua o PDF no projeto: é do
   fabricante.)
2. **Crie `src/midi/<nome>.rs`** com:
   - `pub fn matches(porta: &str) -> bool`: reconhece a porta MIDI pelo nome
     (ex.: `n.contains("DDJ-400")`);
   - uma estrutura que implementa `super::Controller`:
     - `handle(&mut self, msg)`: traduz cada mensagem em comandos do DJ OS (`Cmd::PlayToggle`,
       `Cmd::Cue`, `Cmd::Eq`, `Cmd::Fader`, `Cmd::JogScratch`…, em `src/shared.rs`);
     - `leds(...)`: diz quais LEDs acender a partir do estado (`Snapshot`);
   - `pub fn create(...) -> Box<dyn super::Controller>`.
   O `rev1.rs` é o exemplo completo; funções como `rev1::pad_action` (modos dos pads) podem ser
   reaproveitadas.
3. **Registre no catálogo** `MODELS` em `src/midi/mod.rs`:
   ```rust
   Model { name: "DDJ-400", matches: ddj400::matches, create: ddj400::create },
   ```
4. **Saídas de áudio:** se a controladora tem placa de som com fone separado (4 canais), o
   `src/pw_out.rs` precisa reconhecê-la (`is_rev1`/`find_rev1_in`; hoje só a REV1).
5. **Testes:** copie o jeito de `src/midi/tests.rs` (manda mensagens MIDI e confere os comandos
   que saem) e escreva um guia botão por botão em `docs/mapeamento-<nome>.md`.

## Depois (ideia)

Um modo "aprender" na tela de configurações: aperta o botão na controladora, escolhe a ação na
tela, e o DJ OS monta o mapeamento sozinho, sem programar.
