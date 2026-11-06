# Getting started with amt-cli

This guide walks through a first run — building it yourself or using a
prebuilt binary — a minimal configuration, the expected output of the main
commands, and the interactive KVM console. It also explains why a KVM window
launched with no options closes after a few seconds.

The example addresses and credentials are placeholders. Replace them with the
values for your own AMT device.

## Which path are you on?

- **User** — you just want to run amt-cli: skip section 1 and download the
  prebuilt binary from the
  [Releases page](https://github.com/rimpianto/minisforum.amt-rs/releases)
  instead. Unpack the ZIP, put `amt-cli` (or `amt-cli.exe` on Windows)
  somewhere on your PATH, and continue from
  [section 2](#2-create-a-local-configuration).
  Release archives are built automatically on every `vX.Y.Z` tag for
  windows-amd64, windows-arm64, linux-amd64 and macos-arm64, and published
  with a `SHA256SUMS` file. The linux-arm64 archive is built natively on an
  aarch64 Linux host and attached to the release.
- **Developer** — you want to read or change the code: start at
  [section 1](#1-build-the-executable) and keep reading.

## 1. Build the executable

```sh
git clone https://github.com/rimpianto/minisforum.amt-rs.git
cd minisforum.amt-rs
cargo build --release
```

The executable is:

```text
target/release/amt-cli
```

On Windows the same commands work in PowerShell with the MSVC toolchain; the
executable lands in `target\x86_64-pc-windows-msvc\release\amt-cli.exe` and
configuration, history and screenshots use Windows paths. See
[windows-amd64.md](windows-amd64.md) for the full procedure and the list of
what has been verified on Windows so far.

Check the available commands:

```sh
./target/release/amt-cli --help
```

Expected command list:

```text
machines  List the machines in the config file
power     Change or query the power state
kvm       Open a KVM (remote console) session
bios      Ask the firmware to stop in BIOS setup on the next boot
replay    Rebuild a PNG from captured tiles
sol       Open a Serial-over-LAN session
```

## 2. Create a local configuration

Copy the example outside the repository:

```sh
mkdir -p ~/.config/amt
cp config.example.yaml ~/.config/amt/config.yaml
chmod 600 ~/.config/amt/config.yaml
```

Edit `~/.config/amt/config.yaml` and define at least one machine:

```yaml
default: lab

machines:
  lab:
    host: 192.0.2.10          # documentation address: replace with your AMT IP
    port: 16993
    username: admin
    password: CHANGE_ME
    redirection_port: 16995
```

The real file contains a secret. Do not commit it to the repository.

The historical `~/.amt/config.yaml` path still works. `AMT_CONFIG` can select a
different file, and `AMT_HOST`, `AMT_PORT`, `AMT_USER`, `AMT_PASS`,
`AMT_REDIR_PORT`, and `AMT_MACHINE` can override the selected configuration.

## 3. List the configured machines

Run:

```sh
./target/release/amt-cli machines
```

The command prints its version, the configuration file it actually read, and
the configured targets. An asterisk marks the default machine:

```text
amt-cli 1.3.1 (<commit>)
config: /Users/yourname/.config/amt/config.yaml
* lab  192.0.2.10:16993  wsman 16993  redirection 16995  user admin
```

It exits immediately after printing the list. No password is displayed.

If the file shown is not the one you expected, fix `AMT_CONFIG` or the
configuration search path before debugging network connectivity.

## 4. Read the power state

Run:

```sh
./target/release/amt-cli --machine lab power
```

With no `--state`, this only reads the current state and then exits:

```text
PowerState=2 (On) RequestedPowerState=2 (On)
```

Some firmware does not report `RequestedPowerState`; its absence is not an
error.

Do not use `power --state off` or `power --state reset` as a first test. There
is no confirmation prompt. `power --state on` is a no-op if the host is already
running.

### ME 16 compatibility note

The WS-Man transport supports the legacy TLS behaviour required by ME 16. An
authenticated power-state read is verified on MS-01 (ME 16) and MS-02. Only
power-state reads are claimed on ME 16; power changes have not been commanded
on that firmware.

## 5. Open the KVM console

Run:

```sh
./target/release/amt-cli --machine lab kvm
```

A successful session goes through these stages:

```text
[kvm] connecting to 192.0.2.10:16995
[kvm] tcp ok
[kvm] TLS up, authenticating
[kvm] KVM session established with 192.0.2.10:16995
[kvm] server RFB version: RFB 004.000
[kvm] desktop 1920x1080 (2 byte per pixel)
```

The graphical window opens and the launching terminal becomes an interactive
control console:

```text
amt-rs 1.3.1 - KVM window is live
  machine: lab (use <name> to switch, machines to list)
  status | bios | bios status | bios cancel | boot-bios
  on | off | reset | cycle | quit  (help for this list)
  up/down recall the previous commands
amt-rs [lab]>
```

### Why it exits after five seconds

The default is:

```text
--seconds 5
```

After five seconds the process reaches its deadline, closes the KVM window and
exits. This is expected behaviour, not a crash and not an AMT disconnection.

For a five-minute session:

```sh
./target/release/amt-cli --machine lab kvm --seconds 300
```

For a one-hour session:

```sh
./target/release/amt-cli --machine lab kvm --seconds 3600
```

Do not use `--seconds 0` as an "unlimited" mode: zero is an immediate deadline.
Choose a duration appropriate to the operation.

Use `quit` to leave before the deadline. `reconnect` works only while the
process and its `amt-rs` prompt are still running. If the process has already
exited, run the original `amt-cli ... kvm` command again.

## 6. Use the control console

While the KVM window is live, type commands at the `amt-rs [lab]>` prompt:

| Command | Purpose |
|---|---|
| `help` | Show the available commands |
| `status` | Read the current power state through WS-Man |
| `shot` | Save the current framebuffer without opening a second AMT session |
| `shot /tmp/lab.png` | Save the framebuffer to an explicit path |
| `machines` | List configured machines |
| `use secondary` | Switch the same window to another configured machine |
| `reconnect` | Retry after failed connections while the process is still running |
| `bios` | Request BIOS setup on the next boot |
| `bios status` | Check whether a setup boot is pending |
| `bios cancel` | Cancel a pending setup boot |
| `boot-bios` | Request BIOS setup and reset immediately |
| `on` | Power on the host |
| `off` | Power off the host; read the safety warning below |
| `reset` | Reset the host and let KVM reconnect |
| `cycle` | Request a hardware power cycle; read the safety warning below |
| `quit` | Close the session and exit before the deadline |

Without a path, `shot` saves `amt-shot-<epoch>.png` in
`$XDG_PICTURES_DIR/amt/`, normally `~/Pictures/amt/`.

`off` and `cycle` release the redirection session before issuing the power
request because the firmware refuses them while a client is attached. `reset`
allows the client to reconnect and show POST or setup.

Do not use `off` if the path to the AMT interface depends on the host being
powered on: you could lose the only way to turn it back on remotely.

## 7. Save one frame without opening a window

Use `--screenshot`:

```sh
./target/release/amt-cli --machine lab kvm \
  --seconds 20 --screenshot /tmp/lab.png
```

This mode does not open the interactive window. It connects, decodes a full
frame, writes the PNG and exits. `--seconds 20` is the time budget for the
connection and capture, not a request to keep a window open for 20 seconds.

Expected final message:

```text
screenshot written to /tmp/lab.png (1920x1080)
```

If the console is blank, try waking it with a key:

```sh
./target/release/amt-cli --machine lab kvm \
  --seconds 20 --key shift --screenshot /tmp/lab.png
```

## 8. Open Serial-over-LAN

A timed SOL session uses the same default five-second duration:

```sh
./target/release/amt-cli --machine lab sol --seconds 10
```

It prints the serial console for ten seconds and then exits.

For interactive input:

```sh
./target/release/amt-cli --machine lab sol --interactive
```

With `--interactive`, the `--seconds` value is ignored. SOL must be enabled in
the AMT firmware; KVM working does not prove that SOL is enabled.

## 9. Common connection problems

### `tcp failed: connection timed out` on port 16995

WS-Man on 16993 and redirection on 16995 are separate services. Check the AMT
firmware state:

- `AMT_RedirectionService.ListenerEnabled=true`;
- `CIM_KVMRedirectionSAP.EnabledState` is enabled;
- the management network actually routes traffic to the redirection port.

`IPS_KVMRedirectionSettingData.EnabledByMEBx=true` alone does not prove that
the runtime listener is active.

### `StartRedirectionSession refused, status 2`

Another client already owns the single redirection session. Close another
`amt-cli` window or MeshCommander, then use `reconnect` if your process is still
running, or launch it again.

### The process pauses after failed connections

After three failed attempts the client stops retrying but keeps the control
console available. Correct the network or firmware issue, then type
`reconnect`. If the deadline has already closed the process, rerun the command.

### `power` fails while KVM works

Power operations use WS-Man; KVM uses the redirection service. They have
separate ports, authentication exchanges and TLS paths. A successful KVM
session therefore does not prove that WS-Man is reachable or authenticated;
inspect the WS-Man error independently. Older builds could fail that WS-Man
handshake on ME 16 while KVM worked; the current transport supports the legacy
TLS behaviour required by that firmware.

## 10. Glossary

- **ME (Intel Management Engine)**: firmware subsystem integrated into the
  chipset. It remains available when the operating system is off, provided the
  machine still has power, and supplies the platform's manageability functions.
- **AMT (Intel Active Management Technology)**: the Intel vPro feature set
  exposed by the ME, including remote power control, WS-Man, KVM, SOL and IDER.
- **MEBx (Intel Management Engine BIOS Extension)**: firmware menu used to
  configure ME and AMT, normally opened during POST with Ctrl+P.
- **WS-Man (Web Services for Management)**: SOAP-based management protocol. On
  the documented AMT setups it is reached over HTTPS on TCP port 16993 and uses
  Digest authentication.
- **MEI (Management Engine Interface)**: Linux interface to the ME. A working
  `/dev/mei0` device backed by the `mei_me` driver shows that the local Linux
  communication stack is present.
- **VFIO**: kernel framework for assigning PCI devices away from their normal
  host drivers. In the MS-01 setup, `vfio-pci` selectively reserves the
  I226-LM, preventing `igc` from exposing it as a Linux network interface while
  the other I226-V remains available to Linux.

For hardware validation, evidence collection and additional power-safety
rules, continue with [testing.md](testing.md).
