# Na tela 1 o aparelho abre direto o DJ OS. (Ctrl+Alt+F2 abre um terminal de manutenção.)
if [ "$(tty)" = /dev/tty1 ]; then
    exec /usr/local/bin/dj-os-sessao
fi
