# Building and running amt-cli on Windows 11 AMD64

This guide describes the native Windows build: the binary is produced by the
MSVC toolchain on Windows itself, with no WSL, MinGW or Cygwin involved.

## Prerequisites actually used

| Component | Version used |
|---|---|
| Windows 11 Pro | 10.0.26300 (AMD64) |
| Rust (rustup, `stable-x86_64-pc-windows-msvc`) | rustc/cargo 1.97.1, rustup 1.29.0 |
| Rust target | `x86_64-pc-windows-msvc` |
| Visual Studio Build Tools | 18.11.12328.437, workload *Desktop development with C++* (MSVC v143+ linker and Windows SDK) |
| Git | 2.53.0.windows.3 |

CMake and NASM are **not** required: although `aws-lc-sys` appears in
`Cargo.lock` as a transitive dependency, the TLS stacks actually compiled are
`ring` (amt-redir, tokio-rustls) and reqwest's rustls binding, so the C-based
aws-lc build never runs. If a future dependency activates aws-lc-sys, CMake and
NASM will become mandatory on Windows.

Check that the target is installed:

```powershell
rustup target list --installed   # must include x86_64-pc-windows-msvc
rustup target add x86_64-pc-windows-msvc   # if missing
```

## Building in PowerShell

```powershell
git clone https://github.com/rimpianto/minisforum.amt-rs.git
cd minisforum.amt-rs
cargo build --release --target x86_64-pc-windows-msvc
```

The executable is:

```
target\x86_64-pc-windows-msvc\release\amt-cli.exe
```

A plain `cargo build --release` (host target) works as well and produces
`target\release\amt-cli.exe`; the explicit target triple is what the release
packaging uses.

To format and lint (both available on the stable toolchain):

```powershell
cargo test --workspace
cargo fmt --check          # see "Known gaps" below
cargo clippy --workspace --all-targets -- -D warnings
```

## Configuration on Windows

The config file is looked for in this order:

1. `AMT_CONFIG` (explicit path, wins over everything);
2. `$XDG_CONFIG_HOME\amt\config.yaml` (only when `XDG_CONFIG_HOME` is set);
3. `%APPDATA%\amt\config.yaml` (the Windows default);
4. `%USERPROFILE%\.amt\config.yaml` (the legacy location, still accepted).

`HOME` is never required on Windows; `USERPROFILE` is used instead wherever a
home directory is needed. Copy `config.example.yaml` to
`%APPDATA%\amt\config.yaml` and keep the file mode restrictive:

```powershell
New-Item -ItemType Directory -Force "$env:APPDATA\amt" | Out-Null
Copy-Item config.example.yaml "$env:APPDATA\amt\config.yaml"
icacls "$env:APPDATA\amt\config.yaml" /inheritance:r /grant:r "$env:USERNAME:F"
```

Without a config file the error messages name the exact Windows paths that
were searched:

```
Error: no AMT machine configured: create C:\Users\you\AppData\Roaming\amt\config.yaml
or .amt/config.yaml (see config.example.yaml) or set AMT_HOST
```

The interactive console (`use <name>` history) writes its history file next to
the config actually in use, i.e. `%APPDATA%\amt\history` in the default setup.

## Screenshots

`shot` in the console, and `kvm` without `--screenshot`, write to:

```
%USERPROFILE%\Pictures\amt\amt-shot-<unix-epoch>.png
```

The directory is created on demand. `XDG_PICTURES_DIR` overrides the base
directory when set, on Windows as on the other platforms.

## Examples

```powershell
$exe = ".\target\x86_64-pc-windows-msvc\release\amt-cli.exe"

& $exe machines
& $exe --machine primary power          # read-only power-state query
& $exe --machine primary kvm            # window, closes itself after ~5 s
& $exe --machine primary kvm --seconds 300
& $exe --machine primary kvm --seconds 20 --screenshot out.png
```

On Windows 11 the interactive control console runs in the same terminal
(Windows Terminal and classic conhost both work). Colours and ANSI escapes are
suppressed automatically when stdout is redirected to a file or a pipe.

## Verified on hardware and what is not (yet)

Verified during the Windows port (MS-02 Ultra, ME 15 firmware):

- native build for `x86_64-pc-windows-msvc`; full `cargo test --workspace` green;
- `--version`, `--help`, `machines`, missing-config errors with Windows paths;
- read-only power query over WS-Man (`PowerState=2 (On)`, ReturnValue validated);
- non-interactive KVM capture: `--seconds 20 --screenshot` produced a legible
  1920x1080 PNG of the remote framebuffer;
- native KVM window: title `amt-rs <version> — KVM`, live remote rendering with
  integer scaling; default ~5 s self-close timed at exactly 5 s; a
  `--seconds 300` session stayed live and exited cleanly on schedule;
- keyboard events reach the remote host (verified by visible effects on the
  remote console: escape/`q` quit a remote full-screen program).

Not yet verified on Windows, listed explicitly:

- **mouse pointer delivery** to the remote host (no visible responder was
  available during the test session);
- **window resize / DPI scaling** interaction with the live KVM view;
- **modifier release on focus loss**;
- **manual zoom with real input** (ctrl+wheel / ctrl+plus / ctrl+minus): the
  logic is unit-tested and compiled into this build, but the automated input
  synthesis could not deliver the chord with usable scancodes, so a physical
  pass is still pending; **macOS behaviour is completely unverified** from
  this Windows machine;
- typed-character delivery through synthetic (PostMessage-style) input was
  intermittent in automated testing; physical keyboard input has not shown
  this, but a human-in-the-loop pass over keyboard and mouse is still worth
  doing;
- `cargo fmt --check` and `cargo clippy -D warnings` currently report issues
  in pre-existing code on every platform; the Windows port did not introduce
  new ones.

## The ME 16 WS-Man TLS issue is a separate problem

On MS-01 hardware running Intel ME 16, `amt-cli power` fails during the
initial WS-Man TLS handshake (`received fatal alert: InternalError`) on Linux
as well. Reproduced identically from the Windows build. It is a client/firmware
TLS compatibility issue tracked in [TODO.md](../TODO.md) and
[the README](../README.md#known-compatibility-limitation-me-16-ws-man-tls) —
it predates the Windows port and is not fixed or claimed fixed by it. KVM and
the redirection TLS path are unaffected.
