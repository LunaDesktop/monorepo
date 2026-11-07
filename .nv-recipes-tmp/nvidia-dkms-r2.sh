#!/bin/sh
# Ponawiaj build nvidia-dkms-615.78.08-2 az pojawi sie w repo (globalny lock
# pagsync bywa zajety przez inne buildy – run-pagsync mapuje konflikt na rc=0).
LOG=/var/log/nvidia-dkms-r2.log
PKG=/var/www/repo.paganlinux.eu/stable/nvidia-dkms-615.78.08-2.pag
: > "$LOG"

i=0
while [ "$i" -lt 12 ]; do
    i=$((i + 1))
    if [ -f "$PKG" ]; then
        echo "=== JUZ JEST: $PKG ($(date -Is)) ===" >> "$LOG"
        exit 0
    fi
    echo "===== proba $i $(date -Is)" >> "$LOG"
    /opt/pagan-web-v2/run-pagsync.sh --once --build nvidia-dkms >> "$LOG" 2>&1
    if [ -f "$PKG" ]; then
        echo "=== SUKCES ($(date -Is)) ===" >> "$LOG"
        exit 0
    fi
    sleep 240
done
echo "=== NIE UDALO SIE po $i probach ($(date -Is)) ===" >> "$LOG"
exit 1
