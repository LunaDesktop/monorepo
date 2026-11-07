#!/bin/sh
# Kolejka NVIDIA 615.78.08 – czeka na zwolnienie globalnego locka pagsync,
# potem buduje moduły, userspace i metapakiet (w tej kolejności).
LOG=/var/log/nvidia-615-build.log
: > "$LOG"
echo "=== start $(date -Is) ===" >> "$LOG"

while pgrep -f "/opt/pagan-web-v2/pagsync" >/dev/null 2>&1; do
    echo "czekam na lock ($(date -Is))" >> "$LOG"
    sleep 30
done
sleep 5

for p in nvidia-kernel nvidia-dkms nvidia; do
    echo "===== BUILD $p $(date -Is)" >> "$LOG"
    /opt/pagan-web-v2/run-pagsync.sh --once --build "$p" >> "$LOG" 2>&1
    echo "EXIT $p -> $? $(date -Is)" >> "$LOG"
done

echo "=== KONIEC $(date -Is) ===" >> "$LOG"
