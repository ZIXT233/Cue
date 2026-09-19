# Windows harness startup investigation — 2026-09-19

Non-graphical measurements on the local ARM64 Windows machine. The measurements
below preceded the fixes described in the final section.

## Confirmed: unanswered ConPTY DA1 adds a three-second wait

`examples/pty_startup_probe.rs` uses Que's portable-pty dependency and bundled
ConPTY DLL, with the same 100x30 initial size. Running `cmd /d /c echo
QUE_PTY_READY` isolates terminal initialization from PowerShell, CLI startup,
hooks, WebView, and SSE.

| Trial | No DA1 response | DA1 response |
| --- | ---: | ---: |
| 1 | 3048 ms | 37 ms |
| 2 | 3050 ms | 39 ms |
| 3 | 3053 ms | 41 ms |

The first output is `ESC [ 1 t` (four bytes), followed by `ESC [ c` and terminal
mode sequences. Replying to `ESC [ c` with `ESC [ ? 1 ; 2 c` removes the delay.
Replying only to cursor-position queries does not remove it.

`TerminalPanel.tsx` explicitly filters this DA1 response. Its comment treats
withholding responses as a harmless fallback, but the fallback costs time.
The backend's `firstByteMs` includes the four-byte control sequence, so it does
not measure the time until an interactive CLI draws its interface.

Codex through the npm cmd shim, without Que's per-session CLI arguments:

- No DA1 response: first substantial screen output at 3346 ms.
- DA1 response: first substantial screen output at 425 ms.

These are PTY experiments, not end-to-end measurements of clicking a Que card.
The early CLI trials inherited `NO_COLOR=1`; production removes it. The echo
handshake comparison remains isolated, but CLI screen timings are diagnostic
observations, not a color-enabled production benchmark.

## Confirmed: environment capture is before existing launch timing

`harness/env.rs` invokes `powershell.exe -NoLogo -Command ...` and waits for its
environment dump. The cache lasts 60 seconds. `harness/mod.rs` does this before
the `launch` log and PTY timestamp, so existing first-byte timing excludes it.

Executing the same dump measured 2413 and 1848 ms with the heavy Rust build
finished. Earlier runs during compilation were 5508, 5061, and 8756 ms; those
are load-affected observations, not a reliable idle baseline.

## Cursor-specific delay remains incompletely attributed

Launching the installed Cursor Node entry point in this PTY, with DA1 replies,
`CURSOR_INVOKED_AS`, and the same `NODE_COMPILE_CACHE` path as Que:

- With Que's `--plugin-dir`: first substantial screen output at 10843 and
  8099 ms in two runs.
- Without that argument: 6285 ms in one run.

This is insufficient to assign a stable cost to plugin loading: repetitions,
cache state and other initialization work vary. The plugin hook manifest is
empty; Que actually installs Cursor hooks into the user-level hooks file.
Check whether this otherwise inert plugin is still needed before removing it.
Global user hooks were not disabled during these experiments.

A pre-existing Cursor hook lifecycle log showed 19 ms from process-start to
process-exit. This excludes the CLI runner's process creation and waiting, and
does not establish that all hooks are fast.

## Next implementation priorities

1. Handle the ConPTY startup handshake without depending on visible frontend
   attachment. Distinguish startup queries from later CLI queries and replay;
   do not blindly forward historical responses into a CLI input prompt.
2. Record request start, environment acquisition, hook preparation, PTY creation,
   and frontend attachment separately. Keep first transport byte distinct from
   first visible CLI content.
3. Avoid putting an unnecessary PowerShell environment capture on every cache
   miss, while preserving profile-provided PATH and environment semantics.
4. Repeat controlled Cursor comparisons for the plugin argument and normal
   launcher, then optimize only demonstrated overhead.

## Reproduction

From `src-tauri`:

```powershell
cargo run --release --example pty_startup_probe
cargo run --release --example pty_startup_probe -- --reply-da1
```

The probe also accepts `--reply` for CPR and `-- <executable> <arguments...>`
for a CLI. CLI trials stop after 20 seconds or 2 KB of output and terminate the
spawned child. The 2 KB threshold is an observation limit, not a readiness test.
Use a workspace where starting and stopping a fresh CLI session is acceptable.

## Implemented fixes

- The local Windows PTY reader answers the bundled ConPTY's exact initial
  `ESC [ 1 t ESC [ c` handshake once, before frontend attachment. It handles
  fragmented reads, ignores subsequent queries and leaves the existing
  frontend replay filter in place. Other platforms do not use this handler.
- Windows prewarms the profile-derived environment asynchronously at app startup
  and reuses the last good snapshot while refreshing it in the background
  after 60 seconds. A missing CLI forces a refresh. Captures are serialized,
  with a 10-second subprocess timeout on all platforms. Unix retains its login
  shell and 60-second cache. A card started before prewarming finishes can
  still wait for the first capture. System environment changes not visible to
  the parent/profile may still require an app restart.
- Cursor no longer receives `--plugin-dir` for Que's empty plugin. User-level
  lifecycle hook registration and per-session signal environment are preserved.
- Logs now record request arrival, queue preparation, environment/command
  readiness, hook preparation, PTY creation and the ConPTY handshake. The
  first-byte comment explicitly distinguishes transport bytes from CLI readiness.

Validation: three handshake regression tests passed (all split boundaries,
coalesced modes, and no responses to later application queries). The probe's
`--startup-handshake` mode imports the same handler as the production reader:
the echo trial completed in 47 ms. A real Cursor launch without `--plugin-dir`
still wrote `sessionStart` signals with a session ID into an isolated test sink.
Its first substantial screen output in that run was 5123 ms, but this is not an
end-to-end Que UI benchmark and is not a guarantee for future runs.

## Follow-up: Windows Codex composer colors

This was a separate issue; startup fixes alone did not repair theme switching.

- Que's frontend previously swallowed OSC 10/11/12/4 queries on every local Windows terminal.
- `harness/mod.rs` previously forced local Windows harnesses to dark `COLORFGBG`,
  even when the modern bundled ConPTY is available and the app uses light mode.
- The backend sends theme notifications only after observing DECSET 2031.
  The first capture inherited `NO_COLOR=1` from the diagnostic environment;
  production removes that variable. Its absence of OSC queries was therefore
  not representative. After matching production, the installed Codex 0.155.0
  emitted OSC 10/11 queries, but still no DECSET 2031 in a 20-second capture.
- A Node raw-TTY probe using the same bundled ConPTY emitted `OSC 11 ; ? BEL`.
  The host replied with `ESC ] 11 ; rgb:fdfd/f6f6/e3e3 ESC \\`. The child received
  the complete bytes: `1b5d31313b7267623a666466642f663666362f653365331b5c`.
  A subsequent real Codex run with OSC answers emitted composer background
  `48;2;242;236;217`, versus `48;2;41;41;41` without answers. This confirms that
  both this ConPTY and Codex's startup input reader handle these replies.

Implemented: remove the Windows-only dark environment override; allow live
Codex color queries on modern ConPTY while retaining shell, inbox fallback,
other Windows CLI and historical-replay guards. Startup probes that happened
before frontend attachment are historical and deliberately receive no reply.

Codex 0.155 caches its startup palette (see upstream `terminal_palette.rs` at
`rust-v0.155.0`). For live switching, a Codex-only display filter binds its known
composer RGB tints to xterm palette slot 255. Updating the theme recolors existing
cells without CLI input, restarting sessions or replaying the terminal. Original
indexed color 255 is expanded to its equivalent RGB so it keeps its color.
The tint recognizer covers the bundled Que palettes and the observed Windows
fallback, not arbitrary future Codex styling or all explicit RGB output.
OSC/DCS payloads, unrelated RGB, backend transcripts and offsets stay unchanged.

## Terminal appearance and block glyphs

Settings now persist independent light/dark palette selections (Solarized,
Catppuccin, GitHub, Gruvbox) and a terminal font preference with monospace
fallback. Active and detached panels listen for changes. Background, selection,
ANSI colors and the Codex composer slot all update together. Grok and inbox
ConPTY retain their existing fixed profiles.

Windows now uses the existing WebGL addon with custom glyphs, zero letter
spacing and unit line height. xterm's custom block glyphs fill cell boundaries;
the previous Windows-only DOM renderer used font outlines with visible seams.
GPU failure/context loss retains the DOM fallback (which can still show font
seams). DOM-only caret suppression runs only in that fallback.

Non-graphical checks: `node --test scripts/test-terminal-appearance.cjs` covers
all stream split boundaries, palette changes, original indexed colors,
OSC/DCS payloads, replay reset and preference validation. `npm run build`
passes. No GUI verification was performed.

Palette data: [iTerm2-Color-Schemes](https://github.com/mbadolato/iTerm2-Color-Schemes/tree/master/windowsterminal),
retrieved 2026-09-19; Catppuccin Latte/Mocha, GitHub/GitHub Dark and Gruvbox
Light/Dark. Attribution and MIT license: `terminal-palettes-LICENSE.txt`.
Codex references: [palette cache](https://github.com/openai/codex/blob/rust-v0.155.0/codex-rs/tui/src/terminal_palette.rs),
[composer tint](https://github.com/openai/codex/blob/rust-v0.155.0/codex-rs/tui/src/style.rs).

## Cursor native theme protocol — 2026.09.18-9a7762b

The installed Windows ARM64 bundle's `use-theme-detection` chunk contains empty
subscription strings, but that is not the whole application: a real PTY trace
does emit DECSET 2031 (4470 ms). Inferring that Cursor never subscribes from the
one chunk was incorrect. It queries OSC 11 and consumes the returned RGB.

With the same bundled ConPTY, `--startup-handshake --observe-theme
--reply-colors --theme-cycle --light` observed one process change its background:

- Initial light answer: composer `48;2;241;236;222`.
- Dark notification at 12 seconds, followed by a new OSC 11 query/answer:
  composer `48;2;14;50;59`.
- Light notification at 16 seconds, followed by another query/answer:
  composer `48;2;241;236;222` again.

No `rgb:` or `[?997;` fragments appeared in the captured output. No prompt was
submitted; the diagnostic child was terminated after 20 seconds. This verifies
the native protocol, not the complete Que GUI.

Que now permits live Cursor color queries on modern ConPTY, retains historical
query filtering, and uses Cursor's native subscription rather than a display
color replacement. The subscription observer preserves escape state across PTY
reads and ignores OSC/DCS payloads. Changing palette without changing light/dark
also refreshes subscribed CLIs; font-only changes do not. The inbox ConPTY
fallback and non-subscribers still receive no theme notifications.

Font settings now include per-option font styling plus an always-visible sample
(native dropdowns may ignore option styling), and local font availability labels.
