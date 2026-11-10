# gvm-rs

**Go Version Manager, written in Rust.** A reimplementation of
[moovweb/gvm](https://github.com/moovweb/gvm) that keeps the same UX
(`gvm install`, `gvm use`, `gvm pkgset`, …) but does the heavy lifting in a
Rust binary.

## Why a Rust binary + a shell shim?

`gvm use` has to change the *current shell's* `GOROOT`, `GOPATH` and
`PATH` — a separate process cannot do that for you. Like the original gvm,
gvm-rs therefore ships a small shell function (the **shim**) that is
sourced by your shell. The shim delegates all real work to the `gvm-rs`
binary and `eval`s the shell code the binary prints for environment
changes:

```
~/.gvm/
  gos/<version>/             installed GOROOTs
  packages/<version>/<ps>/   GOPATHs (pkgsets)
  cache/                     downloaded tarballs + clean copies + builds
  versions/<name>            installed names -> version tags
  config/default             default version name
  config/alias               aliases
  scripts/gvm                the sourced shim
  overlays/<version>/        GVM_OVERLAY_PREFIX (vendored native deps)
```

## Installation

```sh
# 1. Install the binary (downloads the cargo-dist release binary for your
#    platform, checksum-verified; falls back to `cargo install` if no
#    release exists yet), then wire up your shell:
bash <(curl -fsSL https://raw.githubusercontent.com/nicolas-moon/gvm-rs/main/binscripts/gvm-installer)

# 2. Open a new shell, then:
gvm install go1.21 --binary   # prebuilt tarball, checksum-verified
gvm use go1.21 --default      # activate + set as default
```

Or manually:

```sh
cargo install gvm-rs
gvm-rs init          # writes the shim and adds the source line to ~/.bashrc / ~/.zshrc
```

Release binaries are built by [cargo-dist](https://axodotdev.github.io/cargo-dist/)
for `x86_64`/`aarch64` on Linux and macOS and published as
`gvm-rs-<target-triple>.tar.xz` (plus `.sha256` sidecars) whenever a
`vX.Y.Z` tag is pushed.

`GVM_ROOT` (default `~/.gvm`) can be overridden to relocate everything.

## Commands

| Command | What it does |
| --- | --- |
| `gvm install <version>` | Install a Go version. Default is a **source build** (bootstrap toolchain auto-detected from installed versions or `PATH`); use `-B`/`--binary` for a prebuilt tarball, `--prefer-binary` to try binary first and fall back to source. Partial versions work: `gvm install 1.21` → latest stable 1.21.x. `latest` and `tip` (master) supported. `-n <name>` installs under a custom name, `-s <url>` from a custom source URL. |
| `gvm use [version]` | Activate a version in the current shell (sets `GOROOT`, `GOPATH`, `PATH`, `GVM_OVERLAY_PREFIX`). `--default` also makes it the default. With no argument, re-activates the current default. |
| `gvm uninstall <name>` | Remove a version (and its pkgsets/overlays when it is the last name for that version). |
| `gvm list` | Installed versions, newest first; the active one (per `$GOROOT`) is marked `=>`. |
| `gvm listall [--all]` | Versions available on go.dev (`--all` includes beta/RC). |
| `gvm diff [--version V]` | Compare the active GOROOT with the clean cached copy: added / removed / modified files (exit code 1 on differences, like `diff`). |
| `gvm pkgset create <name>` | Create a pkgset for the active version and switch to it. |
| `gvm pkgset use <name>` | Switch `GOPATH` to a pkgset (`default` always exists). |
| `gvm pkgset delete <name>` | Delete a pkgset (asks for confirmation, `-y` to skip). |
| `gvm pkgset [current]` | List pkgsets / show the active one. |
| `gvm alias list \| set <n> <t> \| delete <n>` | Version-name aliases. |
| `gvm env` | Show the current gvm environment. |
| `gvm implode [-y]` | Remove the gvm root and everything in it. |
| `gvm-rs init [--shell bash\|zsh] [--rc FILE]` | Install the shim + rc source line (idempotent). |

Binary-only commands (`gvm-rs …` directly): every command above also works
without the shim, except that `use` / `pkgset use|create` then just *print*
the env lines — run them through the sourced `gvm` function to apply them.

### A note on compiling Go 1.5+ from source

Go 1.5+ uses a Go-written toolchain, so building from source needs a working
Go to bootstrap with — same constraint as the original gvm. gvm-rs
auto-detects a bootstrap (newest installed version first, then `go` on
`PATH`) and tells you what to do if none is found:

```sh
gvm install go1.21 --binary   # bootstrap with a prebuilt version
gvm use go1.21
gvm install 1.23              # now the source build works
```

## Differences from the original gvm

- Binary is Rust; the shell layer is a single sourced function instead of a
  large sourced script.
- `--local` pkgsets (repo-local `.gvm_local`) and vendoring helpers
  (`-b`, `-pb`) are not implemented yet (the flags are accepted as no-ops
  for compatibility).
- Binary installs are checksum-verified against go.dev metadata (best
  effort when offline).
- `gvm diff` compares by file size + sha256 instead of shelling out.

## Development

```sh
cargo build
cargo test
cargo clippy --all-targets
```

The unit tests cover version parsing/resolution, name/alias resolution,
env-line generation and pkgset lifecycle against a temp `GVM_ROOT`.

### Releasing

Releases are driven by [cargo-dist](https://axodotdev.github.io/cargo-dist/)
(config in `dist-workspace.toml`, workflow in `.github/workflows/release.yml`):

```sh
git tag v0.1.0
git push origin v0.1.0        # the Release workflow builds all targets and
                              # uploads them to a GitHub Release
```

To also publish to crates.io, run `cargo dist publish` with a
`CARGO_REGISTRY_TOKEN` (the GitHub Release is created by the tag push
regardless).
