# TODO

What is left, why it matters, and what "done" means. Ordered by what I would
do next, not by what is easiest.

## Done after 1.0.0

- [x] Several machines in the config (1.1.0): `--machine`, `machines`,
      `use <name>` in the KVM console, legacy single-machine file still loads

## Done at 1.0.0

- [x] WS-Man power: `on`, `off`, `reset`, `power cycle`, with `ReturnValue`
      validation (a 2xx is not success)
- [x] Power-state query with human names
- [x] Redirection transport, AAAH handshake, TLS on 16995
- [x] KVM video: RAW, RLE and palette-RLE decoders, correct colours
- [x] KVM window: scaling, resolution changes, reconnect after a power action
- [x] Keyboard and mouse, modifiers held so chords work
- [x] Control console: `status`, `bios`, `boot-bios`, `on`, `off`, `reset`,
      `cycle`, history, colours
- [x] Boot to BIOS setup (`AMT_BootSettingData.BIOSSetup` + `SetBootConfigRole`)

## Next

- [ ] **WS-Man TLS compatibility with ME 16.** On the tested MS-01, KVM works
      but `amt-cli power` fails during the initial WS-Man TLS handshake with
      `InternalError`; an independent client succeeds after enabling the legacy
      TLS behaviour required by that firmware. *Done when* the stock Rust
      client completes an authenticated power-state read on ME 16 without a
      separate compatibility wrapper, and the existing MS-02 path still passes.
- [ ] **Dirty-rectangle updates.** Today every new frame is rescanned and
      rescaled entirely. *Done when* a keystroke repaints only the tiles that
      changed and CPU stays low with the window open for an hour.
- [ ] **IDER / USB-R.** Mount an ISO or a floppy image and boot the machine
      from it. *Done when* a machine boots an ISO served over the redirection
      session with no physical media. This is the biggest remaining piece: it
      needs the SCSI-ish protocol on the same session, not just WS-Man.
- [ ] **Clipboard through the KVM data channel.** `ClientCutText` with
      `\0KvmDataChannel`, which MeshCommander implements and we skip.
      *Done when* text copied in the window arrives on the host console.
- [ ] **Relative mouse mode** (RFB 4.0 extension). Absolute pointer is jumpy on
      a graphical host; relative motion is what a compositor expects.
- [ ] **SOL on hardware.** The client is ready; the device is not — it closes
      the session after the start request until SOL/IDER is enabled in MEBx,
      which needs the physical console. *Done when* serial output arrives from
      a real machine.

## Afterwards

- [ ] Windows and ARM64 builds, with the same binary layout as macOS.
      Windows 11 AMD64: native MSVC build done, Windows paths for config,
      history and screenshots done, KVM video/window/keyboard verified on
      MS-02 (see docs/windows-amd64.md). Still open: mouse pointer delivery,
      window resize/DPI and modifier release on focus loss; ARM64 not started.
- [ ] Release packaging: prebuilt binaries on tags, plus a one-line installer.
- [ ] Decoder tests from recorded dumps: `AMT_DUMP` already saves tiles, the
      missing part is turning a few of them into fixtures with expected PNGs.
- [ ] Multi-session handling: tell the user *who* holds the redirection
      session instead of only `status 2`.
- [ ] Latency and frame-rate display in the window title, to make performance
      regressions visible.
- [ ] Config with the password stored outside the YAML (keychain on macOS,
      DPAPI on Windows), so the file can be shared.

## Known limitations, deliberate for now

- One redirection client at a time — that is the device, not the client.
- Power off and power cycle are refused while a client is attached; the console
  releases the session first, which costs a couple of seconds.
- The BIOS screen is shown at its own resolution (640x480, 1024x768) and only
  enlarged by whole factors: crisp rather than smooth.
- No audio, no USB passthrough, no file transfer other than IDE-R.
