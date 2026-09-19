# Terminal compatibility audit — 2026-09-19

Scope: local Windows PTY/theme/rendering, shared frontend reply filters and
Windows hook launch helpers. Source review plus non-graphical checks; this is
not a full audit of every CLI adapter or a claim that all platforms were tested.

## Resolution after the audit

The five findings below describe the original audit state. They have since
been addressed in source:

- Live DA/DSR/CPR replies are allowed. Replay replies and the single backend-
  answered ConPTY startup handshake are suppressed separately.
- Hook commands preserve exact arguments using encoded PowerShell for unsafe
  bare paths; real cmd/Node tests cover special characters, stdin, cwd and exit
  status. Complete owned Codex registrations migrate; edited/incomplete blocks
  are preserved. Arbitrary user .cmd launcher arguments are outside this fix.
- Codex mapping only changes background RGB, binds one cached tint, and has an
  opt-out on all platforms. Matching background RGB elsewhere remains a known
  heuristic limitation. Cursor uses its native theme notification mechanism.
- Bundled ConPTY uses xterm's modern reflow capability floor; inbox fallback
  uses the actual OS build, with a conservative default when unavailable.
- Windows refreshes expired environment snapshots in the background after
  60 seconds; captures on all platforms have a 10-second subprocess timeout.
  This does not bound all queued forced refreshes or arbitrary descendants.

Before the subsequent settings-layout edits, 134 Rust tests, 14 frontend tests
and the frontend build passed. The settings now have a separate Terminal page,
visible Codex switch label, compact previews and the relocated shared font-size
control. Preview and terminal share one size conversion function.

The user then requested no agent-run app, builds or tests. The agent's pending
NSIS build was stopped; no fresh installer is claimed. The settings-layout
changes received source review only. macOS/Linux were not tested natively.

## Original findings (historical)

1. **Live protocol replies are globally suppressed.**
   `TerminalPanel.tsx::isCapabilityReplyOrReport` drops DA1, DA2, DSR and CPR
   for Windows, Unix and SSH, including live output. The measured ConPTY
   startup delay justifies an early host handshake; it does not justify
   disabling all later application probes. Potential consequence: timeouts,
   lost terminal capabilities or incorrect cursor positioning. No complete
   per-CLI protocol matrix has been established. Comments were corrected;
   the broader policy has not yet been changed.

2. **Windows hook quoting solves spaces incompletely.**
   `harness/windows.rs::windows_hook_command` emits unquoted paths when they
   contain no whitespace, and an unquoted `cd /d` path otherwise. Shell
   metacharacters such as `&` and `%` are not addressed by the whitespace
   branch. A node path containing spaces is replaced with PATH lookup `node`
   on the direct branch, so the selected runtime is not guaranteed. These
   paths need real invocation tests with spaces and metacharacters before
   treating the helper as general shell compatibility. No user hook settings
   were modified during this audit.

3. **Codex tint mapping is an adaptation, not native theme support.**
   `codex-composer-colors.ts` recognizes numeric tints, not semantic screen
   regions. Identical colors elsewhere in output can match; future Codex
   versions can change the tint formula; explicit RGB colors elsewhere in
   the TUI do not become theme-aware. The added foreground mapping was too
   broad and has been removed in this audit. A regression now preserves text
   with RGB equal to a composer tint. Background mapping remains in place.

4. **The reported Windows PTY build is a constant.**
   `TerminalPanel.tsx` sets build 26200 even in the inbox-ConPTY fallback.
   xterm 6 uses build 21376 as a reflow/line-wrapping behavior boundary.
   Thus the fallback can claim behavior an older host does not have. The
   actual bundled modern-ConPTY path is distinct; host capability metadata
   should describe that distinction instead of guessing a universal build.

5. **Environment caching is a performance tradeoff with missing bounds.**
   `harness/env.rs` keeps Windows environment snapshots for the app lifetime;
   shell-profile edits are stale until restart or forced command lookup.
   The capture subprocess has no explicit timeout and holds the shared cache
   lock. A hanging profile can therefore block subsequent launches. The
   cache removed measured repeated process startup, but did not solve this
   failure path.

## Evidence-backed fixes to retain

- Initial ConPTY handshake: echo 3.05 s versus roughly 40 ms, once-only prefix
  matching with split-boundary tests. It applies to the bundled host prefix.
- Cursor 2026.09.18: real ConPTY trace subscribes to 2031 and changes composer
  RGB light → dark → light after OSC 11 responses, without echoed protocol
  text. No extra display-color mapping is needed for Cursor.
- Theme subscription parser: state survives fragmented reads, excludes tested
  control-string payloads, and tracks unsubscribe. Notifications remain gated
  for non-subscribers, exited sessions and the fixed Campbell fallback.
- WebGL block glyphs: xterm draws cell-sized blocks; its DOM renderer uses font
  outlines. The GPU fallback can still show font seams; no GUI result is claimed.
- Installer DLL bundling and tray behavior are separate packaging/window fixes,
  not reasons to suppress terminal capabilities.

## Original validation and artifact status (before the fixes above)

- 8 frontend protocol/palette tests passed after restricting Codex mapping.
- 4 standalone Rust theme-parser tests passed.
- `cargo test --release --lib terminal::tests`: 8 passed, including refresh
  notifications and non-subscriber/Campbell guards.
- Frontend production build passed; no graphical verification performed.
- The existing NSIS installer contains the Cursor fix and font preview. The
  final foreground-mapping restriction from this audit is in source/dist only;
  that installer has not been regenerated for the audit-only change.
