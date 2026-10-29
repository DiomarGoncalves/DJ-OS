#!/usr/bin/env python3
"""Controla a VM de teste (iso/testar-qemu.sh com SEM_TELA=1) pelo QMP e pelo terminal serial.

  qmp.py foto <saida.png>            foto da tela
  qmp.py teclas <tecla> [...]        aperta teclas (qcodes do QEMU; "ctrl+alt+f2"; "espera2" pausa 2 s)
  qmp.py clique <x> <y>              clique do mouse na posição (pixels da tela)
  qmp.py serial "<comando>" [seg]    roda um comando no terminal serial e mostra a saída
  qmp.py pendrive <imagem> <id>      liga um pendrive (imagem de disco) com a VM rodando
"""
import json, os, socket, struct, sys, time, zlib

T = os.path.expanduser("~/DJOS/iso/trabalho")


def qmp():
    s = socket.socket(socket.AF_UNIX)
    s.connect(f"{T}/qmp.sock")
    f = s.makefile("rw")
    f.readline()

    def cmd(c, **a):
        f.write(json.dumps({"execute": c, **({"arguments": a} if a else {})}) + "\n")
        f.flush()
        while True:
            r = json.loads(f.readline())
            if "return" in r or "error" in r:
                return r

    cmd("qmp_capabilities")
    return cmd


def foto(out):
    cmd = qmp()
    ppm = out + ".ppm"
    cmd("screendump", filename=ppm)
    time.sleep(0.5)
    d = open(ppm, "rb").read()
    os.remove(ppm)
    parts = d.split(b"\n", 3)
    w, h = map(int, parts[1].split())
    px = parts[3]
    raw = b"".join(b"\0" + px[y * w * 3:(y + 1) * w * 3] for y in range(h))

    def chunk(t, b):
        return struct.pack(">I", len(b)) + t + b + struct.pack(">I", zlib.crc32(t + b) & 0xFFFFFFFF)

    png = b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 2, 0, 0, 0))
    png += chunk(b"IDAT", zlib.compress(raw, 6)) + chunk(b"IEND", b"")
    open(out, "wb").write(png)
    print(f"{out} ({w}x{h})")


def teclas(keys):
    cmd = qmp()
    for k in keys:
        if k.startswith("espera"):
            time.sleep(float(k[6:]))
            continue
        cmd("send-key", keys=[{"type": "qcode", "data": q} for q in k.split("+")], **{"hold-time": 80})
        time.sleep(0.6)


def clique(x, y, w=1280, h=800):
    cmd = qmp()
    ax, ay = int(x * 32767 / (w - 1)), int(y * 32767 / (h - 1))
    move = [{"type": "abs", "data": {"axis": "x", "value": ax}}, {"type": "abs", "data": {"axis": "y", "value": ay}}]
    cmd("input-send-event", events=move)
    time.sleep(0.2)
    for down in (True, False):
        cmd("input-send-event", events=[{"type": "btn", "data": {"down": down, "button": "left"}}])
        time.sleep(0.15)


def serial(command, wait=10.0):
    s = socket.socket(socket.AF_UNIX)
    s.connect(f"{T}/serial.sock")
    s.settimeout(0.5)

    def drain(t):
        out, end = b"", time.time() + t
        while time.time() < end:
            try:
                out += s.recv(65536)
            except socket.timeout:
                pass
        return out

    drain(0.5)
    s.sendall(b"\n")
    drain(1)
    s.sendall(command.encode() + b"; echo FIM-CMD\n")
    out, end = b"", time.time() + wait
    while time.time() < end and out.count(b"FIM-CMD") < 2:
        out += drain(0.5)
    text = out.decode(errors="replace").replace("\r", "")
    # Só a saída: sem a linha do comando e sem os códigos do terminal.
    body = text.split("FIM-CMD\n", 1)[-1].rsplit("FIM-CMD", 1)[0]
    print(body.replace("\x1b[?2004l", "").replace("\x1b[?2004h", "").strip())


def pendrive(img, ident):
    cmd = qmp()
    print(cmd("human-monitor-command", **{"command-line": f"drive_add 0 if=none,id={ident},format=raw,file={img}"}))
    print(cmd("device_add", driver="usb-storage", drive=ident, id=f"dev-{ident}"))


if __name__ == "__main__":
    a = sys.argv[1:]
    if not a:
        sys.exit(__doc__)
    {"foto": lambda: foto(a[1]), "teclas": lambda: teclas(a[1:]), "clique": lambda: clique(int(a[1]), int(a[2])),
     "serial": lambda: serial(a[1], float(a[2]) if len(a) > 2 else 10.0),
     "pendrive": lambda: pendrive(a[1], a[2])}[a[0]]()
