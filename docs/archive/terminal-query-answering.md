# Archived: backend terminal-query answering

Archived 2026-09-16. Superseded by "scheme one": the frontend xterm.js instance
answers terminal queries itself and its replies are written straight back to the
PTY, so Cue no longer keeps a second responder in the backend.

## Why this was removed

Cue ran two independent responders for the same probes:

- backend `deliver()` matched `ESC ] 10/11/12 ; ?` and wrote an OSC colour reply
- frontend xterm.js answered DA1 (`ESC [ c`) on its own `onData`

Neither knew about the other. The frontend half was the visible defect: a CLI
probing device attributes at startup got xterm's `ESC [?1;2c` written into the
PTY as *input*, so the bytes were echoed back onto the prompt as literal text
(`[?1;2c`). Not every CLI waits for DA1 — most read `TERM`/terminfo instead —
which is exactly why an unsolicited reply surfaced as stray keystrokes.

The deeper problem was ownership: terminal capability negotiation belongs to the
terminal emulator, not to a transport layer sitting behind it. `deliver()` is a
byte pump shared by the local PTY and the SSH task; it has no business
maintaining a second opinion about what the canvas colour is.

## What replaced it

`src/components/TerminalPanel.tsx` forwards `onData` to the PTY verbatim. OSC
colour replies and DA/DSR replies all come from xterm, which computes them from
its own live renderer state — so the answers are self-consistent with what the
user actually sees, including after a theme switch or resize.

## What was kept, and why

Not everything here was a duplicate responder:

- `colorfgbg` / `is_dark_colorfgbg` — the `COLORFGBG` env var is a *push* at PTY
  spawn, not a query reply. xterm cannot supply it.
- `observe_theme_notify` — tracks whether the CLI opted into DECSET 2031. Purely
  bookkeeping over the output stream; no reply is generated.
- `theme_change_report` — pushes `CSI ?997;1n/2n` when the *app* theme changes.
  This is a push too: there is no query to answer, and xterm has no knowledge of
  the app-level theme toggle.
- The `LIGHT_*` / `DARK_*` palette constants — still the source of truth for
  `COLORFGBG` and for the frontend's xterm theme, so they stay.

Only `color_query_replies` and `osc_color_reply` were genuinely superseded; both
are reproduced at the bottom of this file for reference.

## Known follow-up

Dropping the backend responder means the canvas colour the CLI is told about is
now whatever xterm was last themed with, not what `apply_canvas_dark` believes.
If a CLI queries OSC 11 *before* the frontend has applied the theme, the answer
will be xterm's default rather than Solarized. Verify on a remote workspace,
where the CLI can start probing before the first theme push lands.

---

## Addendum: capability replies are withheld

Scheme one alone was not enough. Handing DA1 to xterm made Cue answer correctly,
but the reply then travelled `onData` → PTY → and Cursor — the CLI that does the
probing — did not consume it. It echoed `ESC[?1;2c` onto its own prompt as
literal text, so the stray `[?1;2c` survived the refactor in a new form.

`TerminalPanel.tsx` now drops capability replies before they reach the PTY:

- DA1 (`ESC[?1;2c`, `ESC[?6c`)
- DA2 (`ESC[>0;276;0c`, `ESC[>85;95;0c`, `ESC[>83;40003;0c`)
- DSR device status (`ESC[0n`)
- CPR (`ESC[<row>;<col>R`, with or without the `?` private marker)

With no answer, a probing CLI falls back to `TERM` / `COLORFGBG`, which the spawn
environment already carries — the modern path, and the reason these probes are
legacy in the first place.

What deliberately still flows through:

- **OSC 10/11/12 colour replies** — real CLIs do consume these, and xterm computes
  them from live renderer state. (They surface via `_onColor` →
  `triggerDataEvent`, not a constant, so the exact-match list never touches them.)
- **`ESC[?997;1n` / `ESC[?997;2n`** — pushed by `apply_canvas_dark` on a theme
  flip, not an answer to a probe. xterm cannot produce these.
- **Text/cell size (`ESC[4;…t` / `ESC[6;…t`)** — request/notification pairs the CLI
  is actively waiting on.
- **Every arrow/function key** — `ESC[A`..`ESC[D`, `ESC[H`, `ESC[F`, `ESC[5~` are
  also `ESC[`-prefixed, which is why the filter is an exact match plus one narrow
  anchored regex rather than a prefix test.

## Focus reporting: Cue owns it, xterm is silenced

`ESC[I` / `ESC[O` were also added to the block list, for a different reason than
the capability replies: not because they are redundant, but because the two
producers disagree about what they mean.

| Producer | Meaning |
|---|---|
| xterm (`CoreBrowserTerminal.ts:271/295` on real focus/blur, `:1293` on the initial `?1004h` report) | browser textarea gained / lost focus |
| Cue (`sendFocusReport`) | this card is / is not in the queue |

Both write the same two byte sequences. With a CLI that armed DECSET 1004, the
stream carried whichever source fired last, and the queue semantics — the reading
the CLI is actually asking about — were being overwritten by raw browser focus.

`sendFocusReport` is now the single writer. Two supporting facts, checked against
the xterm 6.0.0 sources:

- `CSI I` appears in `InputHandler.ts` only as the *parser* for CHT. xterm never
  emits it as a key, so blocking the sequence cannot swallow a keystroke.
- Tab is `\x09` and Shift+Tab is `\x1b[Z`; neither collides.

Also worth noting: xterm does not expose a switch to disable focus reporting.
`decPrivateModes.sendFocus` (`CoreService.ts:24`) is internal and is only flipped
by the `?1004h`/`?1004l` handlers (`InputHandler.ts:1938`/`:2167`), so filtering
the emitted sequence is the only place Cue can take ownership without rewriting
the output stream on its way into xterm.


---

## Removed code (verbatim)

```rust
pub fn color_query_replies(data: &str, dark: bool) -> Vec<String> {
    let bytes = data.as_bytes();
    let mut replies = Vec::new();
    let mut i = 0;
    while i + 5 < bytes.len() {
        if bytes[i] == 0x1b && bytes[i + 1] == b']' && bytes[i + 2] == b'1' {
            let code = bytes[i + 3];
            if matches!(code, b'0' | b'1' | b'2') && bytes[i + 4] == b';' && bytes[i + 5] == b'?' {
                replies.push(osc_color_reply(code, dark));
            }
        }
        i += 1;
    }
    replies
}

fn osc_color_reply(code: u8, dark: bool) -> String {
    let [r, g, b] = match code {
        b'0' => if dark { DARK_FG } else { LIGHT_FG },
        b'2' => if dark { DARK_CURSOR } else { LIGHT_CURSOR },
        _ => if dark { DARK_BG } else { LIGHT_BG },
    };
    format!("\x1b]1{};rgb:{r:02x}{r:02x}/{g:02x}{g:02x}/{b:02x}{b:02x}\x1b\\", code as char)
}
```

Call site that was removed from `terminal.rs::deliver()`:

```rust
// ConPTY answers its own forwarded queries out of the input stream;
// replying here too would double-answer and leak keystrokes.
let replies = if record.conpty {
    Vec::new()
} else {
    crate::terminal_theme::color_query_replies(&data, record.canvas_dark)
};
let channel = if replies.is_empty() { None } else { Some(record.channel.clone()) };
```

and, further down, the write-back:

```rust
if let Some(channel) = channel {
    for reply in replies {
        channel.write(reply.as_bytes());
    }
}
```

The four tests covering it (`osc_11_query_replies_with_solarized_light_background`,
`osc_10_and_11_in_one_chunk_both_get_answers`, and the two `deliver()` tests
`deliver_answers_osc_11_with_the_solarized_canvas`,
`deliver_tracks_osc_11_background_before_resize`) went with it.
