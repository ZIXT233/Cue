"use client";

import { useEffect, useState, type ReactNode } from "react";
import { useI18n } from "@/hooks/useI18n";
import { useCompletionNotifications } from "@/hooks/useCompletionNotifications";
import { useAudio } from "@/hooks/useAudio";
import { useTheme } from "@/hooks/useTheme";
import { useAttentionMode } from "@/hooks/useAttentionMode";
import { useSubmissionBehavior } from "@/hooks/useSubmissionBehavior";
import { ATTENTION_MODES } from "@/lib/attention-mode";
import { SUBMISSION_BEHAVIORS } from "@/lib/submission-behavior";
import { announceQueueToast } from "@/lib/queue-toast";
import { THEME_OPTIONS } from "@/lib/theme";
import { ThemeIcon } from "./ThemeIcon";
import {
  CHAT_CONTENT_FONT_SIZE_DEFAULT,
  CHAT_CONTENT_FONT_SIZE_MAX,
  CHAT_CONTENT_FONT_SIZE_MIN,
  useChatAppearance,
} from "@/hooks/useChatAppearance";
import type { ShellToolSettingsResponse } from "@/lib/api-types";
import { setLastSettingsSection, type SettingsSection } from "@/lib/settings-navigation";
import { RemoteHostsSettings } from "./RemoteHostsSettings";
import { ConfigButton, ConfigSwitch } from "./SettingsUi";

interface Props {
  cwd: string | null;
  sessionId: string | null;
  initialSection: SettingsSection;
  onClose: () => void;
  onSessionReloaded: () => void;
  quoteSelectionEnabled: boolean;
  onQuoteSelectionChange: (enabled: boolean) => void;
}

export function SettingsSectionIcon({ section, size = 16, strokeWidth = 1.8 }: { section: SettingsSection; size?: number; strokeWidth?: number }) {
  const common = {
    width: size,
    height: size,
    viewBox: "0 0 24 24",
    fill: "none",
    stroke: "currentColor",
    strokeWidth,
    strokeLinecap: "round" as const,
    strokeLinejoin: "round" as const,
    "aria-hidden": true,
    className: "settings-section-icon",
  };
  if (section === "remote-hosts") return <svg {...common}><rect x="4" y="3" width="16" height="7" rx="2" /><rect x="4" y="14" width="16" height="7" rx="2" /><path d="M8 6h.01M8 17h.01M12 6h5M12 17h5" /></svg>;
  return <svg {...common}><path d="M20 7h-9M14 17H5" /><circle cx="7" cy="7" r="3" /><circle cx="17" cy="17" r="3" /></svg>;
}

function GeneralSettings() {
  const { locale, setLocale, supportedLocales, t } = useI18n();
  const notifications = useCompletionNotifications(null);
  const audio = useAudio();
  const [audioBlocked, setAudioBlocked] = useState(false);
  const { preference, setThemePreference } = useTheme();
  const { mode: attentionMode, setMode: setAttentionMode } = useAttentionMode();
  const { mode: submissionBehavior, setMode: setSubmissionBehavior } = useSubmissionBehavior();
  const { fontSize, setFontSize } = useChatAppearance();
  const [shellSettings, setShellSettings] = useState<ShellToolSettingsResponse | null>(null);
  const [shellSaving, setShellSaving] = useState(false);
  const [shellError, setShellError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    void fetch("/api/tools/settings")
      .then(async (response) => {
        const data = await response.json() as ShellToolSettingsResponse & { error?: string };
        if (!response.ok || data.error) throw new Error(data.error ?? `HTTP ${response.status}`);
        if (!cancelled) setShellSettings(data);
      })
      .catch((cause) => {
        if (!cancelled) setShellError(cause instanceof Error ? cause.message : String(cause));
      });
    return () => { cancelled = true; };
  }, []);

  const togglePowerShell = async (enabled: boolean) => {
    setShellSaving(true);
    setShellError(null);
    try {
      const response = await fetch("/api/tools/settings", {
        method: "PUT",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ enabled }),
      });
      const data = await response.json() as ShellToolSettingsResponse & { error?: string };
      if (!response.ok || data.error) throw new Error(data.error ?? `HTTP ${response.status}`);
      setShellSettings(data);
    } catch (cause) {
      setShellError(cause instanceof Error ? cause.message : String(cause));
    } finally {
      setShellSaving(false);
    }
  };

  return (
    <div className="settings-general">
      <h2 className="settings-general-title">{t("settings.general")}</h2>

      <section className="settings-general-section">
        <h3 className="settings-general-heading">{t("settings.appearance")}</h3>
        <div role="radiogroup" aria-label={t("settings.appearance")} className="settings-theme-options">
          {THEME_OPTIONS.map((option) => {
            const selected = preference === option.id;
            return (
              <label key={option.id} className="settings-theme-option">
                <input type="radio" name="theme" value={option.id} checked={selected} onChange={() => setThemePreference(option.id)} className="sr-only" />
                <ThemeIcon preference={option.id} />
                <span className="settings-theme-option-label">{t(option.label)}</span>
              </label>
            );
          })}
        </div>
      </section>

      <section className="settings-general-section">
        <h3 className="settings-general-heading">{t("settings.attentionMode")}</h3>
        <div className="settings-attention-mode-copy">
          <p className="settings-general-description">{t("settings.attentionModeDailyDescription")}</p>
          <p className="settings-general-description">{t("settings.attentionModeFocusDescription")}</p>
        </div>
        <div role="radiogroup" aria-label={t("settings.attentionMode")} className="settings-theme-options settings-attention-mode-options">
          {ATTENTION_MODES.map((option) => {
            const selected = attentionMode === option.id;
            return (
              <label key={option.id} className="settings-theme-option">
                <input
                  type="radio"
                  name="attention-mode"
                  value={option.id}
                  checked={selected}
                  onChange={() => {
                    setAttentionMode(option.id);
                    announceQueueToast(t(option.id === "focus" ? "queue.已切换专注模式说明" : "queue.已切换日常模式说明"));
                  }}
                  className="sr-only"
                />
                <span className="settings-attention-mode-icon" aria-hidden="true">{option.icon}</span>
                <span className="settings-theme-option-label">{t(option.label)}</span>
              </label>
            );
          })}
        </div>
      </section>

      <section className="settings-general-section">
        <h3 className="settings-general-heading">{t("settings.submissionBehavior")}</h3>
        <div className="settings-attention-mode-copy">
          <p className="settings-general-description">{t("settings.submissionKeepInViewDescription")}</p>
          <p className="settings-general-description">{t("settings.submissionCollapseDescription")}</p>
        </div>
        <div role="radiogroup" aria-label={t("settings.submissionBehavior")} className="settings-theme-options settings-attention-mode-options">
          {SUBMISSION_BEHAVIORS.map((option) => {
            const selected = submissionBehavior === option.id;
            return (
              <label key={option.id} className="settings-theme-option">
                <input
                  type="radio"
                  name="submission-behavior"
                  value={option.id}
                  checked={selected}
                  onChange={() => setSubmissionBehavior(option.id)}
                  className="sr-only"
                />
                <span className="settings-attention-mode-icon" aria-hidden="true">{option.icon}</span>
                <span className="settings-theme-option-label">{t(option.label)}</span>
              </label>
            );
          })}
        </div>
      </section>

      <section className="settings-general-section">
        <h3 className="settings-general-heading">{t("settings.notifications")}</h3>
        <div className="settings-chat-options">
          <div className="settings-chat-option settings-chat-switch-option">
            <span>{t("settings.completionNotifications")}</span>
            <div className="settings-chat-option-actions">
              <ConfigButton variant="ghost" size="small" onClick={() => { void notifications.sendTestNotification(); }}>{t("settings.sendTestNotification")}</ConfigButton>
              <ConfigSwitch checked={notifications.enabled} label={t("settings.completionNotifications")} onChange={() => void notifications.toggle()} />
            </div>
          </div>
          <p className="settings-general-description">{t("settings.completionNotificationsDescription")}</p>
          {notifications.status && <p role="status" className="settings-general-error">{notifications.status}</p>}
          <div className="settings-chat-option settings-chat-switch-option">
            <span>{t("settings.notificationSound")}</span>
            <div className="settings-chat-option-actions">
              <ConfigButton variant="ghost" size="small" onClick={() => { void audio.previewSound().then((ok) => setAudioBlocked(!ok)); }}>{t("settings.previewSound")}</ConfigButton>
              <ConfigSwitch checked={audio.soundEnabled} label={t("settings.notificationSound")} onChange={audio.onSoundToggle} />
            </div>
          </div>
          {audioBlocked && <p role="status" className="settings-general-error">{t("settings.audioBlocked")}</p>}
        </div>
      </section>

      <section className="settings-general-section">
        <h3 className="settings-general-heading">{t("settings.chat")}</h3>
        <div className="settings-chat-options">
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
              onChange={(event) => setFontSize(Number(event.target.value))}
            />
          </div>
        </div>
      </section>

      {shellSettings?.isWindows && (
        <section className="settings-general-section">
          <h3 className="settings-general-heading">{t("settings.shellTool")}</h3>
          <p className="settings-general-description">{t("settings.shellToolDescription")}</p>
          <div className="settings-shell-option">
            <span>{t("settings.usePowerShell")}</span>
            <ConfigSwitch
              checked={shellSettings.powerShellEnabled}
              loading={shellSaving}
              label={t("settings.usePowerShell")}
              onChange={(enabled) => void togglePowerShell(enabled)}
            />
          </div>
          {shellError && <p role="alert" className="settings-general-error">{shellError}</p>}
        </section>
      )}

      <section className="settings-general-section">
        <h3 className="settings-general-heading">{t("common.language")}</h3>
        <div role="radiogroup" aria-label={t("common.language")} className="settings-language-options">
          {supportedLocales.map((plugin) => {
            const selected = locale === plugin.id;
            return (
              <button key={plugin.id} type="button" role="radio" aria-checked={selected} onClick={() => setLocale(plugin.id as typeof locale)} className="settings-language-option">
                <span className="settings-language-radio">{selected && <span className="settings-language-radio-dot" />}</span>
                <span className="settings-language-label">{plugin.label}</span>
                <span className="settings-language-code">{plugin.id}</span>
              </button>
            );
          })}
        </div>
      </section>
    </div>
  );
}

export function SettingsPanel({ initialSection, onClose }: Props) {
  const { t } = useI18n();
  const [section, setSection] = useState<SettingsSection>(initialSection === "general" || initialSection === "remote-hosts" ? initialSection : "general");
  const [mountedSections, setMountedSections] = useState<ReadonlySet<SettingsSection>>(() => new Set([section]));
  const sections: { id: SettingsSection; label: string }[] = [
    { id: "general", label: t("settings.general") },
    { id: "remote-hosts", label: t("machines.settings") },
  ];

  useEffect(() => setLastSettingsSection(section), [section]);

  useEffect(() => {
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key !== "Escape" || event.defaultPrevented) return;
      event.preventDefault();
      onClose();
    };
    document.addEventListener("keydown", handleKeyDown);
    return () => document.removeEventListener("keydown", handleKeyDown);
  }, [onClose]);

  const activateSection = (nextSection: SettingsSection) => {
    setMountedSections((current) => new Set(current).add(nextSection));
    setSection(nextSection);
    setLastSettingsSection(nextSection);
  };

  const sectionHost = (id: SettingsSection, content: ReactNode) => mountedSections.has(id) ? (
    <div key={id} hidden={section !== id} className={`settings-section-host${id === "general" ? " is-general" : ""}`}>
      {content}
    </div>
  ) : null;

  return (
    <div role="dialog" aria-modal="true" aria-label={t("settings.title")} onClick={(event) => { if (event.target === event.currentTarget) onClose(); }} className="settings-dialog-backdrop">
      <div className="settings-dialog-surface">
        <div className="settings-dialog-header">
          <div className="settings-dialog-brand">
            <span className="settings-dialog-mark">
              <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
                <path d="m12 3-9 4.5 9 4.5 9-4.5L12 3Z" />
                <path d="m3 12 9 4.5 9-4.5M3 16.5 12 21l9-4.5" />
              </svg>
            </span>
            <span><strong>Cue</strong><small className="settings-dialog-title">{t("settings.title")}</small></span>
          </div>
          <select aria-label={t("settings.title")} value={section} onChange={(event) => activateSection(event.target.value as SettingsSection)} className="settings-mobile-section-picker">
            {sections.map((item) => <option key={item.id} value={item.id}>{item.label}</option>)}
          </select>
          <nav aria-label={t("settings.title")} className="settings-section-tabs">
            {sections.map((item) => (
              <button key={item.id} type="button" className="settings-section-tab" aria-current={section === item.id ? "page" : undefined} onClick={() => activateSection(item.id)}>
                <SettingsSectionIcon section={item.id} />
                <span>{item.label}</span>
              </button>
            ))}
          </nav>
          <button type="button" onClick={onClose} title={t("i18n.close")} aria-label={t("i18n.close")} className="config-close-button settings-dialog-close">×</button>
        </div>
        <main className="settings-dialog-main">
          {sectionHost("remote-hosts", <RemoteHostsSettings />)}
          {sectionHost("general", <GeneralSettings />)}
        </main>
      </div>
    </div>
  );
}
