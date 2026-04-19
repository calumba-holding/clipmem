#!/usr/bin/env bash

set -euo pipefail

if [[ $# -ne 4 ]]; then
  echo "usage: $0 <path> <version> <sha256> <url>" >&2
  exit 1
fi

cask_path="$1"
version="$2"
sha256="$3"
url="$4"

mkdir -p "$(dirname "${cask_path}")"

cat > "${cask_path}" <<EOF
cask "clipmem-app" do
  version "${version}"
  sha256 "${sha256}"

  url "${url}"
  name "Clipmem"
  desc "Menu bar app for local clipboard history"
  homepage "https://github.com/tristanmanchester/clipmem"

  depends_on formula: "clipmem"
  depends_on arch: :arm64
  depends_on macos: ">= :sonoma"

  app "ClipmemMenuBar.app"

  postflight do
    system_command "#{HOMEBREW_PREFIX}/bin/clipmem",
                   args: ["setup"]
  end

  uninstall quit: "io.openclaw.clipmem.menubar"

  zap trash: "~/Library/Preferences/io.openclaw.clipmem.menubar.plist"
end
EOF
