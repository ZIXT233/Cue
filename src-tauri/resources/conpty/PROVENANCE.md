# ConPTY sidecar provenance

Bundled files: `conpty.dll`, `OpenConsole.exe` (win-x64, win-arm64).
`src-tauri/src/conpty.rs` selects the subdir matching the target arch.

- Source: [`Microsoft.Windows.Console.ConPTY.1.24.260710001.nupkg`](https://github.com/microsoft/terminal/releases/download/v1.24.11911.0/Microsoft.Windows.Console.ConPTY.1.24.260710001.nupkg)
- Upstream release: microsoft/terminal **v1.24.11911.0**
- License: MIT (microsoft/terminal)
- Integrity (SHA-256, verified on 2026-09-16 after Authenticode validation):
  - `conpty.dll` → `39FBA2713E2495117B1591AE8C32A3B904BEA7AA66069CF7815E2844C76D75D8`
  - `OpenConsole.exe` → `B7FD936C2668B87B9ECF7B3366DC6568AFC1C6F981874CBA3E955A1C35CF8160`
- win-arm64 (verified 2026-09-19 after Authenticode validation):
  - `win-arm64/conpty.dll` → `DB3D173640B172BAFD42D5B541B638A9AEEC1C7D0E40DD636BF02822A32C912C`
  - `win-arm64/OpenConsole.exe` → `ED7622FD0D3BEDC9AB9F122F5E58EDF0DEF9E7999224F52DD395BA9F54EDBE09`
- All binaries were verified `Valid` and signed by
  `CN=Microsoft Corporation, O=Microsoft Corporation, L=Redmond, S=Washington, C=US`
  via `Get-AuthenticodeSignature` before being committed.

## Why bundled

The inbox ConPTY (kernel32 exports) is frozen with the Windows release
cycle. The nupkg build carries current fixes and enables OSC
10/11/12/17 color-query forwarding (WT 1.22+). Loading is done at startup
by `src-tauri/src/conpty.rs`; portable-pty then picks it up through its
existing sideload path (`LoadLibraryW("conpty.dll")`).

## Upgrade procedure

1. Download the latest **stable** (non-preview) `Microsoft.Windows.Console.ConPTY.*.nupkg`
   from microsoft/terminal releases.
2. Extract per arch: `runtimes/win-<arch>/native/conpty.dll` and
   `build/native/runtimes/<arch>/OpenConsole.exe` into `win-<arch>/`
   subdirectories (`win-x64`, `win-arm64`).
3. Re-run `Get-AuthenticodeSignature` on both (must be `Valid`, Microsoft-signed,
   same signer) and update the hashes above.
