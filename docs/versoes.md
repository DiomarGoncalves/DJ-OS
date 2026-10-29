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

## Lançar uma versão (publicação automática)
1. Suba a versão no `Cargo.toml` (ex.: `version = "0.2.0"`).
2. Mova os itens de "Não lançado" do `CHANGELOG.md` para `## [0.2.0] - <data>`.
3. Teste (`cargo test`; se mexeu na ISO, `iso/construir.sh` e `iso/testar-qemu.sh`).
4. Grave, marque e envie:
   ```bash
   git commit -am "Versão 0.2.0"
   git tag v0.2.0
   git push origin main --tags
   ```
5. O GitHub (`.github/workflows/lancamento.yml`) roda os testes, monta a ISO e o firmware
   assinado e publica em **Releases** com as notas do CHANGELOG. Leva uns 20-30 minutos; o
   andamento aparece na aba **Actions** do repositório.

A marca (`v0.2.0`) precisa ser igual à versão do `Cargo.toml`, senão a publicação para com erro.

### Preparação (uma vez só)
No GitHub: **Settings → Secrets and variables → Actions → New repository secret**:
- Nome: `FIRMWARE_KEY`
- Valor: o conteúdo inteiro de `~/DJOS/chave/firmware.key` (`cat ~/DJOS/chave/firmware.key`,
  incluindo as linhas `-----BEGIN PRIVATE KEY-----` e `-----END PRIVATE KEY-----`).

O segredo só é usado pela montagem; ninguém consegue ler o conteúdo depois de salvo.

## A chave do firmware
- `~/DJOS/chave/firmware.key` é a chave secreta que assina os firmwares. **Fica no PC de quem
  publica e no segredo `FIRMWARE_KEY` do GitHub; nunca entra no repositório.** Guarde uma cópia em lugar seguro: sem ela os aparelhos
  instalados não aceitam mais atualizações (só reinstalando pela ISO).
- `firmware.pub` (a pública) vai dentro do sistema e só serve para conferir.
- Quem montar o DJ OS por conta própria gera uma chave nova: os aparelhos dele aceitam só os
  firmwares dele.
