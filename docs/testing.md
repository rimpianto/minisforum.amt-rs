# Building and testing amt-rs

You need a Rust toolchain, this repository, and network access to the AMT
interface of the target machine. The addresses below are RFC 5737 documentation
values; replace them with your own management addresses.

## 1. Clone and build

```sh
git clone https://github.com/rimpianto/minisforum.amt-rs.git
cd minisforum.amt-rs
cargo build --release
```

The executable is `target/release/amt-cli`.

Useful compile-time checks are:

```sh
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## 2. Configure the target

Copy the example configuration outside the repository:

```sh
mkdir -p ~/.config/amt
cp config.example.yaml ~/.config/amt/config.yaml
chmod 600 ~/.config/amt/config.yaml
```

Edit it and replace every documentation address and `CHANGE_ME` placeholder:

```yaml
default: primary

machines:
  primary:
    host: 192.0.2.10
    port: 16993
    username: admin
    password: CHANGE_ME
  secondary:
    host: 198.51.100.20
    port: 16993
    username: admin
    password: CHANGE_ME
```

`AMT_CONFIG` can point to a different file. The historical
`~/.amt/config.yaml` location and the single-machine `amt:` form remain
supported.

List the configured machines:

```sh
./target/release/amt-cli machines
```

Environment variables override the selected machine:

```sh
export AMT_HOST=192.0.2.10
export AMT_PORT=16993
export AMT_USER=admin
export AMT_PASS='CHANGE_ME'
```

The redirection port defaults to `AMT_PORT + 2`, normally 16993 to 16995.
Override it with `AMT_REDIR_PORT` only when the device requires a different
mapping.

## 3. Check reachability and the redirection service

WS-Man and KVM are different services. A successful connection to 16993 does
not prove that the KVM listener on 16995 is enabled.

Check both ports:

```sh
nc -vz -w 4 192.0.2.10 16993
nc -vz -w 4 192.0.2.10 16995
```

If 16995 times out on multiple clients and from the management network, check
the firmware state:

- `AMT_RedirectionService.ListenerEnabled` should be `true`;
- `CIM_KVMRedirectionSAP.EnabledState` should be enabled;
- `IPS_KVMRedirectionSettingData.EnabledByMEBx=true` alone is not sufficient.

Only one redirection client can hold a session. Close MeshCommander and other
KVM windows before retrying. A busy session normally produces:

```text
StartRedirectionSession refused, status 2
```

After three failed connection attempts the client pauses. Type `reconnect` at
the `amt-rs` prompt after correcting the network or firmware state.

## 4. Read the power state

```sh
./target/release/amt-cli --machine primary power
```

With no `--state`, this is a read-only operation. It should print a numeric
`PowerState` and its human-readable name. `RequestedPowerState` is not exposed
by every firmware and its absence is not an error.

### ME 16 compatibility note

The WS-Man transport supports the legacy TLS behaviour required by ME 16. An
authenticated power-state read passes on MS-01 (ME 16) and MS-02. Only
power-state reads are claimed on ME 16; power changes have not been commanded
on that firmware.

The ignored integration test exercises the same read against a real target:

```sh
AMT_TEST_HOST=192.0.2.10 AMT_TEST_PORT=16993 AMT_TEST_USER=admin AMT_TEST_PASS='CHANGE_ME' cargo test -p amt-wsman --test hardware -- --ignored --nocapture
```

Run it once against an ME 16 target and once against a modern target when
changing the WS-Man transport. The test is ignored by default so normal builds
do not require hardware or credentials.

## 5. Capture one frame without opening a window

```sh
./target/release/amt-cli --machine primary kvm \
  --seconds 20 --screenshot /tmp/amt-screen.png
```

This opens one KVM session, saves a full frame and exits. It verifies TCP, TLS,
Digest authentication, the redirection handshake and video decoding.

A completely black image usually means that the host console is blanked or the
session is not delivering frame updates. Touch the physical keyboard and retry,
or use `--key shift`. Compare the reported desktop size and the saved PNG size.

## 6. Use the interactive KVM window

```sh
./target/release/amt-cli --machine primary kvm --seconds 120
```

Check, in order:

1. text and colours are rendered correctly;
2. keyboard input reaches the host;
3. pointer movement and clicks reach the host;
4. the image does not freeze or tear;
5. POST or setup resolution changes are handled correctly.

The window uses integer scaling to avoid ragged text. On macOS, automated
screen capture of the window requires Screen Recording permission; this is
separate from saving the AMT framebuffer with `--screenshot` or `shot`.

The terminal remains a control console:

```text
status | bios | bios status | bios cancel | boot-bios
on | off | reset | cycle
machines | use <name> | reconnect
shot [file]
quit
```

`machines` lists the configured targets and `use <name>` switches the existing
window to another one. Command history is stored beside the selected AMT
configuration, normally at `~/.config/amt/history`.

## 7. Boot into BIOS setup

```sh
./target/release/amt-cli --machine primary bios --status
./target/release/amt-cli --machine primary bios
./target/release/amt-cli --machine primary bios --cancel
```

To request setup and reset immediately:

```sh
./target/release/amt-cli --machine primary bios --reset
```

The firmware clears the one-shot flag after consuming it. A reset interrupts
the KVM stream; the client should reconnect automatically and show POST or
setup.

## 8. Capture decoder evidence

```sh
AMT_DUMP=/tmp/amt-tiles ./target/release/amt-cli --machine primary kvm \
  --seconds 10 --screenshot /tmp/amt-screen.png
./target/release/amt-cli replay /tmp/amt-tiles --out /tmp/amt-replay.png
```

`AMT_DUMP` records RLE blocks and inflated tile streams for offline diagnosis.
`replay` reconstructs a PNG without another AMT connection. Do not commit
captured frames, dumps or real configuration files to the public repository.

`AMT_DEBUG_XML=1` prints the exact SOAP request. Use it to diagnose envelope
errors, but review output before sharing it because operational requests can
contain site-specific information.

## Power safety

Do not run `power --state off`, `reset`, or `cycle` on a machine whose
availability matters. There is no confirmation prompt.

`off` and `cycle` release the KVM session before issuing the request because
the firmware refuses them while redirection is attached. `reset` can be used
with an attached session and the client reconnects afterward.

If the path to the AMT interface depends on the host being powered on, `off`
can strand the machine with no remote way to power it back on. A dedicated
management link avoids that dependency. `cycle` is different because the
firmware performs the complete off/on sequence itself.
