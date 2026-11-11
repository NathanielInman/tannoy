# AUR packages

| Package | Builds from |
|---|---|
| `tannoy-bin` | The release tarballs (x86_64, aarch64), checked against the release's `SHA256SUMS` |
| `tannoy-git` | `main`, with `cargo build --release` |

Both install `tannoy` and `tannoy-board` to `/usr/bin`. With a package, the
waybar modules and key binds can call `tannoy` without a full path.

## After a release

Once the release workflow has published `vX.Y.Z` with its tarballs:

```sh
packaging/aur/bump.sh X.Y.Z    # pkgver, checksums, pkgrel=1, both .SRCINFO
```

Review the diff, commit it here, then copy each directory's `PKGBUILD` and
`.SRCINFO` into its AUR clone (`ssh://aur@aur.archlinux.org/tannoy-bin.git`,
`tannoy-git.git`) and push. `tannoy-git` only needs its `.SRCINFO` pushed again
when its dependencies change; its `pkgver()` follows the tags.

To check a package before pushing it:

```sh
cd packaging/aur/tannoy-bin && makepkg -f && rm -rf src pkg *.pkg.tar.zst *.tar.gz
```
