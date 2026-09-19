# Remote tmux hook delivery

The supplied report for card 23f61517-ba76-49c0-ac46-5e10cc222c95 shows 668
output chunks, 11 successful writes, no I/O errors, and no hook/title signal.
It establishes that transport is active and Que's state tracking is unconfirmed;
it does not establish whether the model answered a particular prompt.

Source defects addressed:

- The hook wrote raw private OSC 777 into tmux's pane TTY. tmux does not forward
  arbitrary OSC. Hooks now use its DCS passthrough envelope with escaped ESC
  bytes; the Que pane enables allow-passthrough before launching the CLI.
- tmux's title is configured from pane_title for this session, so Codex's title
  fallback reaches Que. User-wide tmux options are not changed.
- Follow-up: set-option uses a pane target, so session options now target
  `=session:` explicitly. Using `=session` here (valid for set-environment)
  looked for a pane/window with the wrong name, preventing status off and
  potentially aborting the following configuration commands. Bottom status
  remains intentionally hidden. This correction received source review only.
- A surviving CLI retains an old QUE_HARNESS_CHANNEL when Que reconnects with
  a new terminal ID. Que updates a session-local tmux environment value on
  attach; each new hook reads that value with a bounded subprocess call. The
  original token remains the fallback if that lookup is unavailable.
- Multiple hook frames delivered in one SSH read are drained together. A Stop
  frame no longer waits indefinitely for a later read after a submit frame.

Reference: [tmux manual](https://github.com/tmux/tmux/blob/master/tmux.1),
allow-passthrough and set-titles-string. tmux 3.3 introduced the passthrough
option; older versions already supported the DCS envelope without that option.

Only source review was performed. No application, tests, builds or remote
commands were run. A parser regression was added but not executed.

Existing sessions started with the old hook keep the old script path and
environment. Reattaching alone cannot retrofit that running CLI. Use a new
tmux card/session for verification, or later restart/resume the CLI deliberately
after saving its session ID. No existing remote session was killed or restarted.
Detached tmux sessions without an attached client do not deliver live OSC;
this change does not add durable offline hook replay.
