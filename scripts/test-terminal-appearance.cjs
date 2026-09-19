// Pure protocol/settings tests; no browser or graphical verification.
const fs = require("node:fs");
const assert = require("node:assert/strict");
const test = require("node:test");
const ts = require("typescript");
require.extensions[".ts"] = (module, filename) => {
  module._compile(ts.transpileModule(fs.readFileSync(filename, "utf8"), {
    compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022, esModuleInterop: true },
    fileName: filename,
  }).outputText, filename);
};
const { CodexComposerColors, codexComposerTheme } = require("../src/lib/codex-composer-colors.ts");
const { paletteTerminalTheme, harnessTerminalTheme, windowsPtyOptions } = require("../src/lib/terminal-theme.ts");
const { TerminalReplyPolicy, isTerminalProtocolReply } = require("../src/lib/terminal-replies.ts");
const { normalizeTerminalAppearance, TERMINAL_PALETTES, terminalFontFamily } = require("../src/lib/terminal-appearance.ts");

test("real ConPTY Codex composer sequences survive every split boundary", () => {
  const input = "\x1b[0m\x1b[39;48;2;41;41;41m› hello\x1b[49m";
  const expected = "\x1b[0m\x1b[39;48;5;255m› hello\x1b[49m";
  for (let i = 0; i <= input.length; i++) {
    const filter = new CodexComposerColors();
    assert.equal(filter.feed(input.slice(0, i)) + filter.feed(input.slice(i)), expected);
  }
  const filter = new CodexComposerColors();
  assert.equal([...input].map(c => filter.feed(c)).join(""), expected);
});
test("theme changes rebind existing composer cells in both directions", () => {
  const light = codexComposerTheme(paletteTerminalTheme(false), false);
  const dark = codexComposerTheme(paletteTerminalTheme(true), true);
  assert.equal(light.extendedAnsi[239], "#f2ecd9");
  assert.equal(dark.extendedAnsi[239], "#1e444e");
  assert.notEqual(light.extendedAnsi[239], dark.extendedAnsi[239]);
  assert.equal(new CodexComposerColors().feed("\x1b[48;2;242;236;217m"), "\x1b[48;5;255m");
});
test("preserve original indexed 255, unrelated truecolor, cursor and text", () => {
  const original = "\x1b[38;5;255;48;5;255m\x1b[48;2;20;30;40m\x1b[2;8H中文 😀";
  assert.equal(new CodexComposerColors().feed(original), "\x1b[38;2;238;238;238;48;2;238;238;238m\x1b[48;2;20;30;40m\x1b[2;8H中文 😀");
});
test("foreground matching a composer tint remains ordinary text", () => {
  const input = "\x1b[38;2;41;41;41mtext\x1b[38;2;242;236;217mtext";
  assert.equal(new CodexComposerColors().feed(input), input);
});
test("OSC clipboard/title and DCS payloads are never rewritten", () => {
  for (const start of ["\x1b]52;c;", "\x1b]0;", "\x1bP", "\x1b_"]) {
    const input = `${start}\x1b[48;2;41;41;41m\x1b\\\x1b[48;2;41;41;41m`;
    const expected = `${start}\x1b[48;2;41;41;41m\x1b\\\x1b[48;5;255m`;
    const filter = new CodexComposerColors();
    assert.equal([...input].map(c => filter.feed(c)).join(""), expected);
  }
});
test("replay reset drops partial escape state", () => {
  const filter = new CodexComposerColors();
  filter.feed("\x1b[48;2;"); filter.reset();
  assert.equal(filter.feed("restored\x1b[48;2;41;41;41m"), "restored\x1b[48;5;255m");
});
test("all palette variants have complete colors; fixed profiles remain fixed", () => {
  for (const palette of TERMINAL_PALETTES) for (const dark of [true, false]) {
    const theme = paletteTerminalTheme(dark, palette);
    for (const key of ["background", "foreground", "cursor", "black", "red", "green", "yellow", "blue", "magenta", "cyan", "white", "brightBlack", "brightRed", "brightGreen", "brightYellow", "brightBlue", "brightMagenta", "brightCyan", "brightWhite"]) {
      assert.match(theme[key], /^#[a-f\d]{6}$/i, `${palette} ${dark} ${key}`);
    }
    assert.equal(harnessTerminalTheme(dark, "campbell", palette).background, "#0C0C0C");
    assert.equal(harnessTerminalTheme(dark, "grok", palette).background, "#131313");
  }
});
test("invalid saved preferences migrate safely; fonts have monospace fallbacks", () => {
  assert.deepEqual(normalizeTerminalAppearance(null), { light: "solarized", dark: "solarized", font: "default", codexAdaptiveBackground: true });
  assert.deepEqual(normalizeTerminalAppearance({ light: "missing", dark: "github", font: "bad; css" }), { light: "solarized", dark: "github", font: "default", codexAdaptiveBackground: true });
  assert.equal(normalizeTerminalAppearance({ codexAdaptiveBackground: false }).codexAdaptiveBackground, false);
  assert.equal(terminalFontFamily("Consolas", "monospace"), '"Consolas", monospace');
});

test("live probes work on all transports; replay cannot answer old queries", () => {
  for (const modern of [false, true]) {
    const policy = new TerminalReplyPolicy(modern);
    policy.observeOutput("application output");
    for (const reply of ["\x1b[?1;2c", "\x1b[>0;276;0c", "\x1b[0n", "\x1b[12;34R", "\x1b[?12;34R"]) {
      assert.equal(policy.suppress(reply, false, false), false);
      assert.equal(policy.suppress(reply, true, false), true);
    }
    for (const input of ["hello", "\x1b[A", "\x1b[200~paste\x1b[201~", "text\x1b[0n"]) {
      assert.equal(policy.suppress(input, true, false), false);
    }
  }
});
test("only the backend-answered initial ConPTY DA1 is suppressed once", () => {
  const prefix = "\x1b[1t\x1b[c";
  for (let split = 0; split <= prefix.length; split++) {
    const policy = new TerminalReplyPolicy(true);
    policy.observeOutput(prefix.slice(0, split));
    policy.observeOutput(prefix.slice(split));
    assert.equal(policy.suppress("\x1b[0n", false, false), false);
    assert.equal(policy.suppress("\x1b[?1;2c", false, false), true);
    assert.equal(policy.suppress("\x1b[?1;2c", false, false), false);
  }
  const fallback = new TerminalReplyPolicy(false);
  fallback.observeOutput(prefix);
  assert.equal(fallback.suppress("\x1b[?1;2c", false, false), false);
  const ordinary = new TerminalReplyPolicy(true);
  ordinary.observeOutput("hello" + prefix);
  assert.equal(ordinary.suppress("\x1b[?1;2c", false, false), false);
});
test("focus reporting has one owner without suppressing other live replies", () => {
  const policy = new TerminalReplyPolicy(false);
  for (const reply of ["\x1b[I", "\x1b[O"]) {
    assert.equal(policy.suppress(reply, false, true), true);
    assert.equal(policy.suppress(reply, false, false), false);
  }
  assert.equal(policy.suppress("\x1b[0n", false, true), false);
});

test("color replies use the protocol lane without classifying ordinary input", () => {
  for (const value of ["\x1b]10;rgb:8383/9494/9696\x1b\\", "\x1b]11;rgb:0000/2b2b/3636\x07", "\x1b]4;255;rgb:eeee/eeee/eeee\x1b\\", "\x1b[12;34R"]) {
    assert.equal(isTerminalProtocolReply(value), true);
  }
  for (const value of ["hello", "\x1b[A", "\x1b[200~\x1b]11;rgb:0000/2b2b/3636\x07\x1b[201~", "\x1b]11;?\x07"]) {
    assert.equal(isTerminalProtocolReply(value), false);
  }
});
test("PTY reflow capabilities distinguish bundled and inbox engines", () => {
  assert.equal(windowsPtyOptions(false, { dataset: {} }), undefined);
  assert.equal(windowsPtyOptions(true, { dataset: { windowsBuild: "19045" } }).buildNumber, 21376);
  assert.equal(windowsPtyOptions(true, { dataset: { conptyFallback: "true", windowsBuild: "19045" } }).buildNumber, 19045);
  assert.equal(windowsPtyOptions(true, { dataset: { conptyFallback: "true" } }).buildNumber, 17763);
});
test("composer binds one cached tint and retains it across replay reset", () => {
  const filter = new CodexComposerColors();
  assert.equal(filter.feed("\x1b[48;2;41;41;41m"), "\x1b[48;5;255m");
  filter.reset();
  assert.equal(filter.feed("\x1b[48;2;242;236;217m"), "\x1b[48;2;242;236;217m");
  assert.equal(filter.feed("\x1b[48;2;41;41;41m"), "\x1b[48;5;255m");
});
test("BEL inside DCS does not turn its payload into terminal instructions", () => {
  const input = "\x1bPpayload\x07\x1b[48;2;41;41;41m\x1b\\";
  const filter = new CodexComposerColors();
  assert.equal([...input].map(c => filter.feed(c)).join(""), input);
});
