# Testes

## Testes automáticos
```bash
cargo test            # motor de áudio, mapeamento da REV1, browser, configurações…
```
Testes com o PipeWire de verdade (manuais, precisam de um WAV curto):
```bash
DJOS_WAV_TESTE=trecho.wav cargo test -- --ignored saida_virtual
DJOS_WAV_TESTE=trecho.wav cargo test -- --ignored desvia
```

## Máquina virtual (`iso/testar-qemu.sh`)
| Comando | O que faz |
|---|---|
| `iso/testar-qemu.sh` | liga a ISO numa janela (BIOS antiga) |
| `iso/testar-qemu.sh uefi` | idem com UEFI |
| `SEM_TELA=1 …` | sem janela, para testes automáticos (QMP e terminal serial) |
| `VGA=vmware\|std\|cirrus` | outras placas de vídeo (vmware ≈ VMSVGA do VirtualBox) |
| `SEM_SOM=1` | sem placa de som |
| `PENDRIVE=a.img,b.img` | pendrives USB virtuais |
| `DISCO=d.img INSTALAR=vda` | instala sozinho no disco virtual (e desliga) |
| `DISCO=d.img SO_DISCO=1` | liga pelo disco instalado |
| `sudo REV1=1 …` | passa a DDJ-REV1 de verdade para dentro da VM |

Com `SEM_TELA=1`, o `iso/teste/qmp.py` controla a VM:
```bash
iso/teste/qmp.py foto tela.png                 # foto da tela
iso/teste/qmp.py teclas down tab 1 z           # teclas (qcodes do QEMU)
iso/teste/qmp.py clique 90 722                 # clique do mouse
iso/teste/qmp.py serial 'dj-os-diagnostico'    # comando no terminal (usuário dj)
iso/teste/qmp.py pendrive pd.img p1            # liga um pendrive com a VM rodando
```

## No aparelho
`Ctrl+Alt+F2` abre um terminal de manutenção (`Ctrl+Alt+F1` volta). `dj-os-diagnostico` mostra
pendrives, som, vídeo e USB.
