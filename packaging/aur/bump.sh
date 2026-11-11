#!/usr/bin/env bash
# Points tannoy-bin at a published release: sets pkgver, takes the checksums
# from the release's SHA256SUMS, resets pkgrel and rewrites .SRCINFO.
#   packaging/aur/bump.sh 0.2.0
set -euo pipefail
ver=${1:?usage: bump.sh <version without v>}
here=$(cd "$(dirname "$0")" && pwd)
sums=$(gh release download "v$ver" -R NathanielInman/tannoy -p SHA256SUMS -O -)
sum() { awk -v f="tannoy-$1-unknown-linux-gnu.tar.gz" '$2 == f { print $1 }' <<<"$sums"; }
x86=$(sum x86_64)
arm=$(sum aarch64)
[[ -n $x86 && -n $arm ]] || { echo "v$ver has no SHA256SUMS for both targets" >&2; exit 1; }
pkg=$here/tannoy-bin/PKGBUILD
sed -i -e "s/^pkgver=.*/pkgver=$ver/" -e "s/^pkgrel=.*/pkgrel=1/" \
  -e "s/^sha256sums_x86_64=.*/sha256sums_x86_64=('$x86')/" \
  -e "s/^sha256sums_aarch64=.*/sha256sums_aarch64=('$arm')/" "$pkg"
for p in tannoy-bin tannoy-git; do
  (cd "$here/$p" && makepkg --printsrcinfo > .SRCINFO)
done
echo "tannoy-bin is at $ver; review the diff, then push both to the AUR"
