# amt-rs — native Intel AMT/vPro management in Rust

Intel Active Management Technology (AMT), available on supported Intel vPro
systems, provides hardware-level remote management independently of the
installed operating system. It can be used to recover an unresponsive machine,
change its power state, access the BIOS/UEFI setup, view and control the remote
console through KVM, or use Serial-over-LAN. Unlike conventional remote-desktop
software, these functions do not require an agent running inside the operating
system.

`amt-rs` is a native Rust client for these capabilities. It provides power
control, KVM with keyboard and mouse input, boot-to-BIOS setup, Serial-over-LAN,
and management of multiple machines from a single configuration file.
Everything is available through one `amt-cli` binary, without requiring a
Node.js runtime or a browser.

For now, `amt-rs` is developed as a subsystem of the MeshCommander project
rather than as a replacement for the entire project. MeshCommander remains the
protocol reference: `amt-rs` was written by observing and documenting the wire
protocols, not by embedding MeshCommander's browser interface or Node.js
process.

Development and initial hardware validation used a Minisforum MS-02 Ultra, with
additional validation on a Minisforum MS-01. However, `amt-rs` is intended for
supported Intel AMT/vPro hardware in general and is not tied to a particular
Minisforum model.

## Glossary

- **Intel vPro**: Intel's platform for business PCs. AMT is available on
  supported vPro systems; the hardware must support AMT and the feature must be
  provisioned before `amt-rs` can connect.
- **ME (Intel Management Engine)**: firmware subsystem integrated into the
  chipset. It remains available when the operating system is off, provided the
  machine still has power, and supplies the platform's manageability functions.
- **AMT (Intel Active Management Technology)**: the Intel vPro feature set
  exposed by the ME, including remote power control, WS-Man, KVM, SOL and IDER.
- **Out-of-band management**: remote management through firmware rather than
  through an agent in the installed operating system. AMT can remain available
  when the operating system is not running, provided the machine has power and
  its management network remains reachable.
- **KVM (Keyboard, Video, Mouse)**: remote-console access that carries video
  from the managed machine and keyboard and mouse input back to it. On systems
  with KVM support, this includes POST and BIOS/UEFI screens as well as the
  operating-system console.
- **SOL (Serial-over-LAN)**: redirection of a serial console over the network.
  It is useful for text-based firmware and operating-system consoles when SOL
  is enabled and the managed system is configured to use a serial console.
- **BIOS/UEFI and POST**: BIOS or UEFI is the system firmware that initializes
  and boots a machine; POST is its power-on self-test. The `bios` command asks
  AMT to stop at the firmware setup screen on the next boot; `boot-bios` also
  resets the machine immediately.
- **MEBx (Intel Management Engine BIOS Extension)**: firmware menu used to
  configure ME and AMT, normally opened during POST with Ctrl+P.
- **WS-Man (Web Services for Management)**: SOAP-based management protocol. On
  the documented AMT setups it is reached over HTTPS on TCP port 16993 and uses
  Digest authentication.
- **IDER / USB-R (IDE Redirection / USB Redirection)**: AMT mechanisms for
  redirecting remote boot media or USB devices. They are not currently
  implemented by `amt-rs`.
- **MEI (Management Engine Interface)**: Linux interface to the ME. A working
  `/dev/mei0` device backed by the `mei_me` driver shows that the local Linux
  communication stack is present.
- **VFIO**: kernel framework for assigning PCI devices away from their normal
  host drivers. In the MS-01 setup, `vfio-pci` selectively reserves the
  I226-LM, preventing `igc` from exposing it as a Linux network interface while
  the other I226-V remains available to Linux.
- **MeshCommander**: the Intel AMT management project of which `amt-rs` is
  currently a subsystem and whose wire protocols are the reference for this
  implementation.

## Two ways to get amt-cli

**You just want to use it (recommended for most people)** — download a
prebuilt binary from the
[Releases page](https://github.com/rimpianto/minisforum.amt-rs/releases):
grab the ZIP for your platform, unpack it, and run `amt-cli`. No Rust
toolchain, no compiler, nothing to install. Then follow
[Getting started](docs/getting-started.md) from the configuration section
onwards.

**You want to read or change the code** — clone the repository and build it
yourself:

```sh
git clone https://github.com/rimpianto/minisforum.amt-rs.git
cd minisforum.amt-rs
cargo build --release
```

## How releases are made

Releases are fully automated. A `Release amt-cli X.Y.Z` commit bumps the
version, the `vX.Y.Z` tag is pushed, and the release workflow builds binaries
for windows-amd64, windows-arm64, linux-amd64 and macos-arm64, assembles the
archives, and publishes them together with a `SHA256SUMS` file - no manual
packaging. The linux-arm64 archive is built natively on an aarch64 Linux host
and attached to the release.

Everything a developer needs — build prerequisites for Windows/macOS/Linux,
test commands, hardware verification recipes — lives in
[docs/getting-started.md](docs/getting-started.md) and
[docs/testing.md](docs/testing.md).

## State at 1.3.1

| Area | State |
|---|---|
| Power: `on` (2), `off` (8), `reset` (10), `power cycle` (5) | Works on the MS-02 Ultra, with `ReturnValue` validated |
| Power-state query with human-readable names | Works on MS-02 Ultra and MS-01 (ME 16); see the TLS compatibility note below |
| Redirection transport and AAAH handshake | Works on MS-02 and MS-01 hardware |
| KVM video: RAW, RLE, palette-RLE and ZLIB decoders | Works: 1920x1080 RGB565, legible text and correct colours |
| KVM window, keyboard and mouse | Works: integer scaling, resolution changes and automatic reconnection |
| Manual KVM zoom and pan | Works on macOS with ctrl+wheel, ctrl+plus/minus/0 and ctrl+left-drag; Windows build contains the same logic, with physical-input verification still pending |
| Control console in the launching terminal | `status`, `bios`, `boot-bios`, `on`, `off`, `reset`, `cycle`, `machines`, `use`, `reconnect`, `shot`, `quit` |
| Boot to BIOS setup | Works on the MS-02 Ultra without pressing Delete during POST |
| Multiple configured machines | Works: global `--machine`, environment selection and `use <name>` inside KVM |
| SOL | Works against the mock; hardware requires SOL/IDER to be enabled in the firmware |
| Native Windows 11 AMD64 build | Builds with the MSVC toolchain; config, history and screenshots use Windows paths. KVM video and window verified on MS-02; mouse, resize and DPI still to verify — see [docs/windows-amd64.md](docs/windows-amd64.md) |
| IDER / USB-R | Not implemented |

### ME 16 WS-Man TLS compatibility

WS-Man uses a vendored OpenSSL connector with TLS 1.2 and
`SSL_OP_LEGACY_SERVER_CONNECT` for firmware that does not implement RFC 5746
secure renegotiation. An authenticated `amt-cli power` read passes on an MS-01
running ME 16 and on the MS-02 Ultra. Only the power-state read is claimed on
ME 16; power changes have not been commanded on that firmware.

## Quick start

```sh
git clone https://github.com/rimpianto/minisforum.amt-rs.git
cd minisforum.amt-rs
cargo build --release
./target/release/amt-cli --help
```

The executable is `target/release/amt-cli`.

Common commands:

```sh
./target/release/amt-cli machines
./target/release/amt-cli --machine primary power
./target/release/amt-cli --machine primary kvm
./target/release/amt-cli --machine primary kvm \
  --seconds 20 --screenshot out.png
./target/release/amt-cli --machine primary sol --seconds 10
./target/release/amt-cli --machine primary bios --status
```

Do not test `off`, `reset` or `cycle` on a machine whose availability matters.
A bare `power` command only reads the current state; `power --state on` is a
no-op when the machine is already running.

For a guided first run, including expected output, the default five-second KVM
timeout, interactive commands and troubleshooting, read
[**Getting started with amt-cli**](docs/getting-started.md).

## Configuration

The default configuration is `~/.config/amt/config.yaml`, or
`$XDG_CONFIG_HOME/amt/config.yaml` when `XDG_CONFIG_HOME` is set. `AMT_CONFIG`
can point to another file. The historical `~/.amt/config.yaml` location is
still accepted.

Keep the real file outside the repository and set mode `0600`. Copy
[`config.example.yaml`](config.example.yaml) as a starting point:

```yaml
default: primary

machines:
  primary:
    host: 192.0.2.10             # RFC 5737 documentation address: replace it
    port: 16993                  # WS-Man over TLS
    username: admin
    password: CHANGE_ME

  secondary:
    host: 198.51.100.20          # RFC 5737 documentation address: replace it
    port: 16993
    username: admin
    password: CHANGE_ME
    redirection_port: 16995      # optional; defaults to WS-Man port + 2
    mac: 02:00:00:00:00:20       # optional inventory label; never sent
```

Select a machine with `--machine`, `AMT_MACHINE`, or the `default:` entry:

```sh
./target/release/amt-cli machines
./target/release/amt-cli --machine secondary power
./target/release/amt-cli --machine secondary kvm
```

Inside an interactive KVM session, `machines` lists the configured targets and
`use <name>` switches the same window to another machine.

The old single-machine form still loads:

```yaml
amt:
  host: 192.0.2.10
  port: 16993
  username: admin
  password: CHANGE_ME
```

Environment overrides are applied after machine selection:

- `AMT_HOST`
- `AMT_PORT`
- `AMT_USER`
- `AMT_PASS`
- `AMT_REDIR_PORT`
- `AMT_MACHINE`

For example:

```sh
AMT_HOST=192.0.2.10 AMT_PASS=CHANGE_ME ./target/release/amt-cli power
```

## KVM and redirection

With TLS, WS-Man normally uses port 16993 and redirection uses **16995**;
without TLS the pair is 16992 and 16994. Set `redirection_port` or
`AMT_REDIR_PORT` when a device differs.

Only one redirection client can hold a session. Another `amt-cli` window or a
MeshCommander session can therefore produce:

```text
StartRedirectionSession refused, status 2
```

Close the other session before reconnecting.

If the redirection port times out on every client, check the firmware state,
not only IP routing:

- `AMT_RedirectionService.ListenerEnabled` must be `true`;
- `CIM_KVMRedirectionSAP.EnabledState` must report an enabled state;
- KVM being allowed in MEBx (`EnabledByMEBx=true`) does not by itself prove
  that the runtime listener is enabled.

An unreachable AMT is retried three times before the session pauses. Use
`reconnect` at the `amt-rs` prompt, or `use <name>` to select another target.

## Interactive controls

The terminal that launches KVM remains a control console:

```text
status | bios | bios status | bios cancel | boot-bios
on | off | reset | cycle
machines | use <name> | reconnect
shot [file]
quit
```

`shot` writes the current framebuffer without opening a second AMT session.
Without an explicit path it saves `amt-shot-<epoch>.png` under
`$XDG_PICTURES_DIR/amt/`, normally `~/Pictures/amt/` (`%USERPROFILE%\Pictures\amt\`
on Windows).

Inside the KVM window the picture can be zoomed and panned without touching
the host:

```text
ctrl+mouse wheel   step along the zoom ladder: fit, then 1:1, 2:1, 3:1, 4:1, 6:1
ctrl+plus / ctrl+minus   same, from the keyboard (numpad keys work too)
ctrl+left-drag     pan the zoomed picture around
ctrl+0             back to the plain fit-to-window picture, centred
```

Zoom steps past the fit are INTEGER multiples of the framebuffer pixel, so
nearest-neighbour text stays crisp at every step — fractional scales shred
thin console fonts, which is exactly what a 1080p text console needs to stay
readable. While zoomed in, ctrl+left-drag pans; the drag is intercepted
locally and never reaches the host, and the pointer is mapped through the
zoomed/panned rectangle, so clicks stay where they land. The shortcuts are
matched on physical keys, so they work the same on every keyboard layout, on
Windows and macOS alike.

`off` and `cycle` release the redirection session first because the firmware
refuses those actions while a client is attached. A reset keeps the session
usable and lets the window reconnect for POST or setup.

## Repository layout

```text
.
├── crates/
│   ├── amt-wsman/   # WS-Man: SOAP envelopes, Digest authentication, CIM
│   ├── amt-redir/   # AMT redirection: TCP/TLS, AAAH, KVM and SOL transport
│   ├── amt-kvm/     # RFB video decoders and keyboard/mouse input
│   └── amt-cli/     # The amt-cli executable and interactive console
├── docs/
│   ├── getting-started.md  # First run, expected output, timeout and glossary
│   ├── windows-amd64.md    # Native Windows 11 AMD64 build and verification state
│   └── testing.md          # Hardware validation and evidence collection
├── ref/
│   └── kvm-protocol.md
├── tests/
├── config.example.yaml
├── Cargo.toml
├── LICENSE
└── TODO.md
```

## WS-Man details that still matter

Intel AMT can reject an envelope that merely looks valid, often with HTTP 400
and `SchemaValidationError` rather than a useful field-level diagnostic.

The power envelope validated on the original firmware has these properties:

- HTTPS on port 16993 and HTTP Digest authentication;
- no `<w:OptionSet/>` and no `<w:OperationTimeout>`;
- a one-line XML body, with nothing before `<?xml`;
- `Content-Type: application/soap+xml;charset=UTF-8`;
- a complete `ManagedElement` EPR containing `Address` and
  `ReferenceParameters`;
- `ResourceURI` for `CIM_ComputerSystem` and selectors
  `CreationClassName=CIM_ComputerSystem` and `Name=ManagedSystem`;
- a fresh `MessageID` and the CIM method URI, not the generic WS-Man method
  namespace.

The `<a:To>` value in a local configuration must contain the real AMT address.
A documentation-only example is:

```text
https://192.0.2.10:16993/wsman
```

For a power action, HTTP success is not enough: the response must contain
`<ReturnValue>0</ReturnValue>`.

`AMT_BootSettingData` is in the Intel namespace
(`intel.com/wbem/wscim/1/amt-schema/1`), is a singleton addressed without
selectors, and requires a complete writable instance on Put. Requesting
`BIOSSetup=true` also requires `CIM_BootService.SetBootConfigRole` for the next
boot to consume the setting.

## Testing and evidence

See [docs/testing.md](docs/testing.md) for build, configuration, safe hardware
checks, KVM evidence collection and power-safety rules.

Useful decoder diagnostics:

```sh
AMT_DUMP=/tmp/tiles ./target/release/amt-cli kvm \
  --seconds 10 --screenshot /tmp/screen.png
./target/release/amt-cli replay /tmp/tiles --out /tmp/replay.png
```

`AMT_DEBUG_XML=1` prints the exact SOAP request for comparison with a known
working envelope. Be aware that debug output and captured frames are local
operational artefacts, not files to commit indiscriminately.

## Roadmap and versioning

The remaining work and acceptance criteria are in [TODO.md](TODO.md). Near-term
items include dirty-rectangle rendering, IDER, and clipboard support.

Versions through 1.0.0 followed the commit count. Starting with 1.0.0, version
numbers are assigned deliberately. Release state is recorded with annotated
tags; this repository preserves the v1.0.0 through v1.2.4 tags and the
`amt-rs/` project history.

## License

GPL-3.0. See [LICENSE](LICENSE).

MeshCommander (Apache-2.0) is used as a protocol reference. Any code ported
from it in the future must retain the corresponding attribution and Apache-2.0
terms.
