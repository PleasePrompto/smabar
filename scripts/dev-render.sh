#!/usr/bin/env bash
# Starts the normal dev entrypoint with an explicit Linux WebKit renderer.
set -euo pipefail
cd "$(dirname "$0")/.."

mode=${1:-}
if [[ -z "$mode" ]]; then
  PS3="Rendering auswählen: "
  select choice in "Auto" "NVIDIA" "Software"; do
    case "$REPLY" in
      1) mode=auto; break ;;
      2) mode=nvidia; break ;;
      3) mode=software; break ;;
      *) echo "Bitte 1, 2 oder 3 wählen." >&2 ;;
    esac
  done
fi

software_vars=(
  LIBGL_ALWAYS_SOFTWARE
  __EGL_VENDOR_LIBRARY_FILENAMES
  WEBKIT_SKIA_ENABLE_CPU_RENDERING
)

case "$mode" in
  auto)
    exec env -u WEBKIT_WEB_RENDER_DEVICE_FILE -u GDK_GL \
      -u "${software_vars[0]}" -u "${software_vars[1]}" -u "${software_vars[2]}" \
      scripts/dev.sh
    ;;
  nvidia)
    nvidia_node=""
    for uevent in /sys/class/drm/renderD*/device/uevent; do
      [[ -f "$uevent" ]] || continue
      if grep -qx 'DRIVER=nvidia' "$uevent"; then
        nvidia_node="/dev/dri/$(basename "$(dirname "$(dirname "$uevent")")")"
        break
      fi
    done
    if [[ -z "$nvidia_node" || ! -e "$nvidia_node" ]]; then
      echo "error: kein NVIDIA-Render-Node unter /dev/dri gefunden" >&2
      exit 1
    fi
    echo "NVIDIA: $nvidia_node"
    exec env -u GDK_GL -u "${software_vars[0]}" -u "${software_vars[1]}" -u "${software_vars[2]}" \
      WEBKIT_WEB_RENDER_DEVICE_FILE="$nvidia_node" \
      scripts/dev.sh
    ;;
  software)
    mesa_vendor=/usr/share/glvnd/egl_vendor.d/50_mesa.json
    if [[ ! -f "$mesa_vendor" ]]; then
      echo "error: $mesa_vendor fehlt; installiere das Mesa-EGL-Paket" >&2
      exit 1
    fi
    exec env -u WEBKIT_WEB_RENDER_DEVICE_FILE -u GDK_GL \
      LIBGL_ALWAYS_SOFTWARE=1 \
      __EGL_VENDOR_LIBRARY_FILENAMES="$mesa_vendor" \
      WEBKIT_SKIA_ENABLE_CPU_RENDERING=1 \
      scripts/dev.sh
    ;;
  *)
    echo "Aufruf: $0 [auto|nvidia|software]" >&2
    exit 2
    ;;
esac
