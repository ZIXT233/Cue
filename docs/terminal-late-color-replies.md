# Intermittent color replies in Codex input

The supplied Windows Codex 0.155.0 report for terminal
b1f68348c05a4f9294da65fbb782c321 shows one listener, no dropped replay bytes,
three successful writes and a final 50-byte write. The screenshot shows OSC
10/11 RGB responses rendered as input. Two ST-terminated RGB responses with
four digits per channel total 50 bytes. The report has no query/response
timestamps, so this supports response leakage without proving its exact timing.

Source changes:

- Recognized protocol replies bypass requestAnimationFrame and the ordinary
  serialized input/resize/upload queue. Each response is dispatched immediately.
- Outstanding response requests are aborted when their terminal view is disposed.
- Incremental catch-up has reset=false and is historical just like reset=true.
  Previously only reset=true suppressed replies; reconnect could answer an old
  query after the CLI stopped waiting. Live events omit reset.
- Normal application queries remain enabled; no blanket color-response filter
  or input-text deletion was introduced.

No app, build or test was run. A reply-classification regression was added but
not executed. This addresses identified delay/replay paths; it is not proof
that the intermittent case is eliminated. Frontend parsing and transport still
add latency, and this does not establish a single reply owner across separate
views of the same PTY.
