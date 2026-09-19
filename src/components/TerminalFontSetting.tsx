import { useEffect, useState, type ReactNode } from "react";
import { useI18n } from "@/hooks/useI18n";
import { TERMINAL_FONTS, terminalFontFamily } from "@/lib/terminal-appearance";

export function TerminalFontSetting({ font, fontSize, onChange, children }: {
  font: string;
  fontSize: number;
  onChange: (font: string) => void;
  children?: ReactNode;
}) {
  const { t } = useI18n();
  const [available, setAvailable] = useState<Record<string, boolean>>({});
  const family = (name: string) => terminalFontFamily(name, "var(--font-mono, monospace)");
  useEffect(() => {
    let disposed = false;
    // FontFace local() checks the requested face without mistaking a successful
    // fallback for an installed font (document.fonts.check does the latter).
    if (typeof FontFace === "undefined") return;
    void Promise.all(TERMINAL_FONTS.filter(name => name !== "default").map(async name => {
      try { await new FontFace("que-font-availability", `local("${name}")`).load(); return [name, true] as const; }
      catch { return [name, false] as const; }
    })).then(entries => { if (!disposed) setAvailable(Object.fromEntries(entries)); });
    return () => { disposed = true; };
  }, []);

  return <>
    <label className="settings-terminal-font">
      <span className="sr-only">{t("settings.terminalFont")}</span>
      <select value={font} style={{ fontFamily: family(font) }} onChange={event => onChange(event.target.value)}>
        {TERMINAL_FONTS.map(name => <option key={name} value={name} style={{ fontFamily: family(name) }}>
          {name === "default" ? t("settings.terminalFontDefault") : name}
          {available[name] === false ? ` · ${t("settings.terminalFontUnavailable")}` : ""}
        </option>)}
      </select>
    </label>
    {children}
    <div className="settings-terminal-font-preview" aria-label={t("settings.terminalFontPreview")}>
      <span className="settings-terminal-font-preview-label">{t("settings.terminalFontPreview")}</span>
      <pre style={{ fontFamily: family(font), fontSize }}>
        {"Aa Bb 0123456789 · Il1 O0 {} [] =>\n"}{t("settings.terminalFontPreviewText")}
      </pre>
    </div>
    <p className="settings-general-description" aria-live="polite">
      {t(available[font] === false ? "settings.terminalFontFallback" : "settings.terminalFontDescription")}
    </p>
  </>;
}
