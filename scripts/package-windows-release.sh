#!/usr/bin/env bash
set -euo pipefail

target="${1:?target triple is required}"
profile="${2:?profile directory is required}"
dist_dir="${3:?dist directory is required}"
version="${4:?version is required}"

server_bin="target/${target}/${profile}/winctl-mcp-server.exe"
tray_bin="target/${target}/${profile}/winctl-tray.exe"
launcher_bin="target/${target}/${profile}/winctl-launcher.exe"

if [[ ! -f "${server_bin}" ]]; then
  echo "missing server binary: ${server_bin}" >&2
  exit 1
fi

rm -rf "${dist_dir}"
mkdir -p \
  "${dist_dir}/bin" \
  "${dist_dir}/assets" \
  "${dist_dir}/assets/brand" \
  "${dist_dir}/docs" \
  "${dist_dir}/examples" \
  "${dist_dir}/scripts" \
  "${dist_dir}/logs" \
  "${dist_dir}/captures" \
  "${dist_dir}/artifacts" \
  "${dist_dir}/memory" \
  "${dist_dir}/models" \
  "${dist_dir}/manifests" \
  "${dist_dir}/replays" \
  "${dist_dir}/exports"

cp "${server_bin}" "${dist_dir}/bin/"
if [[ ! -f "${tray_bin}" ]]; then
  echo "missing tray binary: ${tray_bin}" >&2
  exit 1
fi
if [[ ! -f "${launcher_bin}" ]]; then
  echo "missing launcher binary: ${launcher_bin}" >&2
  exit 1
fi
cp "${tray_bin}" "${dist_dir}/bin/"
cp "${launcher_bin}" "${dist_dir}/bin/"

webview2_arch="x64"
case "${target}" in
  i686-*) webview2_arch="x86" ;;
  aarch64-*) webview2_arch="arm64" ;;
esac
webview2_loader="$(
  find "target/${target}/${profile}/build" \
    -path "*/webview2-com-sys-*/out/${webview2_arch}/WebView2Loader.dll" \
    -print -quit 2>/dev/null || true
)"
if [[ -n "${webview2_loader}" && -f "${webview2_loader}" ]]; then
  cp "${webview2_loader}" "${dist_dir}/bin/"
fi

cp README.md "${dist_dir}/"
cp ROADMAP.md "${dist_dir}/"

cp assets/brand/winctl.ico "${dist_dir}/assets/"
cp assets/brand/* "${dist_dir}/assets/brand/"

cp \
  docs/INDEX.md \
  docs/windows-runbook.md \
  docs/CONFIGURATION.md \
  docs/DISTRIBUTION.md \
  docs/DASHBOARD.md \
  docs/CLIENT_CONFIGS.md \
  docs/DIRECTORY_LAYOUT.md \
  docs/TROUBLESHOOTING.md \
  docs/MINILM_EMBEDDINGS.md \
  docs/MEMORY_BACKUP.md \
  "${dist_dir}/docs/"

cp \
  scripts/run-windows-integration.ps1 \
  scripts/diagnose-winctl-capture.ps1 \
  scripts/diagnose-winctl-mcp.ps1 \
  scripts/build-windows-msi.ps1 \
  scripts/install-winctl-mcp.ps1 \
  scripts/verify-windows-signatures.ps1 \
  "${dist_dir}/scripts/"

cp examples/* "${dist_dir}/examples/"

git_revision="$(git rev-parse --short HEAD 2>/dev/null || printf unknown)"
built_at_utc="$(date -u +%Y-%m-%dT%H:%M:%SZ)"

cat > "${dist_dir}/VERSION.txt" <<EOF
name=winctl-mcp
version=${version}
target=${target}
profile=${profile}
git_revision=${git_revision}
built_at_utc=${built_at_utc}
EOF

cat > "${dist_dir}/RELEASE.json" <<EOF
{
  "name": "winctl-mcp",
  "version": "${version}",
  "target": "${target}",
  "profile": "${profile}",
  "git_revision": "${git_revision}",
  "built_at_utc": "${built_at_utc}",
  "binaries": [
    "bin/winctl-mcp-server.exe",
    "bin/winctl-tray.exe",
    "bin/winctl-launcher.exe"
  ],
  "entrypoints": {
    "http": "winctl-mcp-server.exe serve --transport http --listen 127.0.0.1:8765",
    "stdio": "winctl-mcp-server.exe serve --transport stdio",
    "control": "winctl-launcher.exe run",
    "self_test": "winctl-mcp-server.exe self-test windows-list"
  },
  "docs": [
    "docs/DISTRIBUTION.md",
    "docs/CLIENT_CONFIGS.md",
    "docs/TROUBLESHOOTING.md"
  ]
}
EOF

(
  cd "${dist_dir}"
  find . -type f ! -name CHECKSUMS.sha256 -print0 \
    | sort -z \
    | while IFS= read -r -d '' file; do
        sha256sum "${file#./}"
      done
) > "${dist_dir}/CHECKSUMS.sha256"

echo "Packaged Windows release in ${dist_dir}"
echo "Checksums written to ${dist_dir}/CHECKSUMS.sha256"
