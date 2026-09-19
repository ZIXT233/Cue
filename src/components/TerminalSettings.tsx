import { useEffect, useState } from "react";
import { useI18n } from "@/hooks/useI18n";
import { useChatAppearance, CHAT_CONTENT_FONT_SIZE_DEFAULT, CHAT_CONTENT_FONT_SIZE_MIN, CHAT_CONTENT_FONT_SIZE_MAX } from "@/hooks/useChatAppearance";
import { useTerminalBackground } from "@/hooks/useTerminalBackground";
import { TERMINAL_BACKGROUND_OPTIONS } from "@/lib/terminal-background";
import { readTerminalAppearance, saveTerminalAppearance, terminalFontSize, TERMINAL_PALETTES, TERMINAL_PALETTE_LABELS, TERMINAL_APPEARANCE_EVENT, TERMINAL_APPEARANCE_KEY, type TerminalAppearance, type TerminalPalette } from "@/lib/terminal-appearance";
import { paletteTerminalTheme } from "@/lib/terminal-theme";
import { TerminalFontSetting } from "./TerminalFontSetting";
import { ConfigButton, ConfigSwitch } from "./SettingsUi";

export function TerminalSettings() {
  const { t } = useI18n();
  const { background, setBackground } = useTerminalBackground();
  const { fontSize, setFontSize } = useChatAppearance();
  const [appearance, setAppearance] = useState(readTerminalAppearance);
  const update = (patch: Partial<TerminalAppearance>) => {
    const next = { ...readTerminalAppearance(), ...patch };
    setAppearance(next);
    saveTerminalAppearance(next);
  };
  useEffect(() => {
    const sync = (event: Event) => {
      if (event instanceof StorageEvent && event.key !== TERMINAL_APPEARANCE_KEY && event.key !== null) return;
      setAppearance(readTerminalAppearance());
    };
    window.addEventListener(TERMINAL_APPEARANCE_EVENT, sync);
    window.addEventListener("storage", sync);
    return () => {
      window.removeEventListener(TERMINAL_APPEARANCE_EVENT, sync);
      window.removeEventListener("storage", sync);
    };
  }, []);

  return <div className="settings-general settings-terminal">
    <h2 className="settings-general-title">{t("settings.terminal")}</h2>
    <section className="settings-general-section" aria-labelledby="terminal-colors-heading">
      <h3 id="terminal-colors-heading" className="settings-general-heading">{t("settings.terminalColors")}</h3>
      <div role="radiogroup" aria-label={t("settings.terminalBackground")} className="settings-theme-options settings-terminal-mode">
        {TERMINAL_BACKGROUND_OPTIONS.map(option => <label key={option.id} className="settings-theme-option">
          <input type="radio" name="terminal-background" value={option.id} checked={background === option.id} onChange={() => setBackground(option.id)} className="sr-only" />
          <span className="settings-theme-option-label">{t(option.label)}</span>
        </label>)}
      </div>
      <div className="settings-terminal-palettes">
        {(["light", "dark"] as const).map(mode => {
          const preview = paletteTerminalTheme(mode === "dark", appearance[mode]);
          return <label key={mode} className="settings-terminal-palette">
            <span>{t(mode === "light" ? "settings.terminalLightPalette" : "settings.terminalDarkPalette")}</span>
            <select value={appearance[mode]} onChange={event => update({ [mode]: event.target.value as TerminalPalette })}>
              {TERMINAL_PALETTES.map(id => <option key={id} value={id}>{TERMINAL_PALETTE_LABELS[id]}</option>)}
            </select>
            <span className="settings-terminal-swatch" style={{ background: preview.background, color: preview.foreground }} aria-hidden="true">
              <span>Aa</span>
              {[preview.red, preview.green, preview.yellow, preview.blue, preview.magenta, preview.cyan].map((color, i) => <i key={i} style={{ background: color }} />)}
            </span>
          </label>;
        })}
      </div>
    </section>
    <section className="settings-general-section" aria-labelledby="terminal-font-heading">
      <h3 id="terminal-font-heading" className="settings-general-heading">{t("settings.terminalFont")}</h3>
      <TerminalFontSetting font={appearance.font} fontSize={terminalFontSize(fontSize)} onChange={font => update({ font })}>
          <div className="settings-chat-option settings-chat-range-option">
            <div className="settings-chat-range-header">
              <label htmlFor="settings-chat-content-font-size">{t("settings.chatContentFontSize")}</label>
              <output htmlFor="settings-chat-content-font-size">{fontSize}px</output>
              <ConfigButton
                variant="ghost"
                size="small"
                className="settings-chat-reset"
                title={t("settings.resetChatContentFontSize")}
                aria-label={t("settings.resetChatContentFontSize")}
                disabled={fontSize === CHAT_CONTENT_FONT_SIZE_DEFAULT}
                onClick={() => setFontSize(CHAT_CONTENT_FONT_SIZE_DEFAULT)}
              >
                <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
                  <path d="M3 12a9 9 0 1 0 9-9 9.75 9.75 0 0 0-6.74 2.74L3 8M3 3v5h5" />
                </svg>
              </ConfigButton>
            </div>
            <input
              id="settings-chat-content-font-size"
              type="range"
              min={CHAT_CONTENT_FONT_SIZE_MIN}
              max={CHAT_CONTENT_FONT_SIZE_MAX}
              step={1}
              value={fontSize}
              onChange={event => setFontSize(Number(event.target.value))}
            />
          </div>
      </TerminalFontSetting>
    </section>
    <section className="settings-general-section settings-terminal-adaptation" aria-labelledby="terminal-codex-heading">
      <div>
        <h3 id="terminal-codex-heading" className="settings-general-heading">{t("settings.codexAdaptiveBackground")}</h3>
        <p className="settings-general-description">{t("settings.codexAdaptiveBackgroundDescription")}</p>
      </div>
      <ConfigSwitch checked={appearance.codexAdaptiveBackground} label={t("settings.codexAdaptiveBackground")} onChange={codexAdaptiveBackground => update({ codexAdaptiveBackground })} />
    </section>
  </div>;
}
