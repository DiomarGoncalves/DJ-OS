# Versões, ISO e firmware

## Números de versão
- A versão do sistema fica **só** no `Cargo.toml` (`version = "0.1.0"`), no formato
  *maior.menor.correção*:
  - **correção** (0.1.**1**): conserto, sem mudar como se usa;
  - **menor** (0.**2**.0): recurso novo;
  - **maior** (**1**.0.0): mudança grande (ex.: primeira versão pública estável).
- Cada montagem acrescenta a data e a hora: `0.1.0-20261029.1245`. É esse número que aparece em
  CONFIG → SISTEMA e no nome dos arquivos. O aparelho avisa ao instalar um firmware mais antigo.

## O que sai de cada montagem (`iso/construir.sh`)
Em `~/DJOS/iso/`:

| Arquivo | Para quê |
|---|---|
| `dj-os-<versão>.iso` | Instalar do zero ou testar sem instalar (pendrive / Ventoy) |
| `dj-os-firmware-<versão>.djos` | Atualizar um aparelho já instalado, pelo pendrive |
| `dj-os.iso` | Atalho para a ISO mais recente (usado pelo `iso/testar-qemu.sh`) |

A ISO só precisa ser distribuída para instalações novas; quem já tem o DJ OS instalado atualiza
pelo firmware.

## Lançar uma versão
1. Suba a versão no `Cargo.toml`.
2. Mova os itens de "Não lançado" do `CHANGELOG.md` para a versão nova, com a data.
3. `iso/construir.sh` (gera a ISO e o firmware assinado).
4. Teste na máquina virtual (`iso/testar-qemu.sh`, veja `docs/desenvolvimento/testes.md`).
5. `git commit` e `git tag v<versão>` (ex.: `git tag v0.2.0`).

## A chave do firmware
- `~/DJOS/chave/firmware.key` é a chave secreta que assina os firmwares. **Fica só no PC de quem
  publica e nunca entra no repositório.** Guarde uma cópia em lugar seguro: sem ela os aparelhos
  instalados não aceitam mais atualizações (só reinstalando pela ISO).
- `firmware.pub` (a pública) vai dentro do sistema e só serve para conferir.
- Quem montar o DJ OS por conta própria gera uma chave nova: os aparelhos dele aceitam só os
  firmwares dele.
