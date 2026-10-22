#!/usr/bin/env bash
# Fait charger une sauvegarde VS par le serveur dédié (copie jetable), puis l'arrête.
# Usage : tools/vs/validate.sh <monde.vcdbs> [secondes]
# Affiche les erreurs du serveur ; la copie traitée reste dans $WORK/Saves pour inspection.
set -euo pipefail
SAVE=$(realpath "$1"); WAIT=${2:-60}
VS=${VS_DIR:-$HOME/.local/share/vintagestory}
WORK=${WORK:-$(mktemp -d /tmp/odasio-validate.XXXX)}
mkdir -p "$WORK/Mods" "$WORK/Saves"
cp "$SAVE" "$WORK/Saves/world.vcdbs"
python3 -I - "$(dirname "$0")/template-serverconfig.json" "$WORK" <<'PY'
import json, socket, sys
c = json.load(open(sys.argv[1])); w = sys.argv[2]
s = socket.socket(); s.bind(("127.0.0.1", 0)); c["Port"] = s.getsockname()[1]; s.close()
c["ModPaths"] = ["Mods", w + "/Mods"]
c["WorldConfig"]["SaveFileLocation"] = w + "/Saves/world.vcdbs"
json.dump(c, open(w + "/serverconfig.json", "w"), indent=1)
PY
cd "$VS"
(sleep "$WAIT"; echo /stop) | DOTNET_ROOT=$HOME/.dotnet timeout $((WAIT + 120)) ./VintagestoryServer --dataPath "$WORK" > "$WORK/run.log" 2>&1 || true
echo "Copie validée : $WORK/Saves/world.vcdbs"
grep -E "\[(Server )?Error\]|Exception|Failed deserializing|Discarding" "$WORK/run.log" | head -20 || true
grep -E "spawn chunks|now running|World saved|Stopped the server" "$WORK/run.log"
