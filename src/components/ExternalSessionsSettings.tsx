"use client";

import { useEffect, useState } from "react";
import { useI18n } from "@/hooks/useI18n";
import { ProviderIcon } from "./ProviderIcon";
import { ConfigSwitch } from "./SettingsUi";
import { providerIconId } from "@/lib/harness/catalog";

export type FormSupportStatus = "supported" | "unsupported" | "in_progress";

export interface ExternalHarnessConfig {
  id: string;
  name: string;
  vendor: string;
  iconId: string;
  forms: {
    cli: FormSupportStatus;
    desktop: FormSupportStatus;
    vscode: FormSupportStatus;
  };
}

const EXTERNAL_HARNESSES: ExternalHarnessConfig[] = [
  {
    id: "codex",
    name: "Codex",
    vendor: "OpenAI",
    iconId: "openai",
    forms: {
      cli: "supported",
      desktop: "unsupported",
      vscode: "supported",
    },
  },
  {
    id: "cursor",
    name: "Cursor Agent",
    vendor: "Cursor",
    iconId: "cursor",
    forms: {
      cli: "supported",
      desktop: "supported",
      vscode: "unsupported",
    },
  },
  {
    id: "antigravity",
    name: "Antigravity",
    vendor: "Google",
    iconId: "google",
    forms: {
      cli: "supported",
      desktop: "supported",
      vscode: "supported",
    },
  },
  {
    id: "grok",
    name: "Grok",
    vendor: "xAI",
    iconId: "grok",
    forms: {
      cli: "supported",
      desktop: "unsupported",
      vscode: "unsupported",
    },
  },
  {
    id: "claude",
    name: "Claude Code",
    vendor: "Anthropic",
    iconId: "anthropic",
    forms: {
      cli: "supported",
      desktop: "unsupported",
      vscode: "supported",
    },
  },
  {
    id: "opencode",
    name: "OpenCode",
    vendor: "OpenCode",
    iconId: "opencode",
    forms: {
      cli: "supported",
      desktop: "supported",
      vscode: "supported",
    },
  },
  {
    id: "codebuddy",
    name: "CodeBuddy",
    vendor: "Tencent",
    iconId: "anthropic",
    forms: {
      cli: "supported",
      desktop: "unsupported",
      vscode: "unsupported",
    },
  },
  {
    id: "pi",
    name: "Pi / OMP",
    vendor: "Pi",
    iconId: "pi",
    forms: {
      cli: "supported",
      desktop: "unsupported",
      vscode: "unsupported",
    },
  },
];

function FormCapsule({
  formLabel,
  status,
  statusLabel,
}: {
  formLabel: string;
  status: FormSupportStatus;
  statusLabel: string;
}) {
  return (
    <span className={`external-form-capsule is-${status}`}>
      <span className="external-capsule-form">{formLabel}</span>
      <span className="external-capsule-status">{statusLabel}</span>
    </span>
  );
}

export function ExternalSessionsSettings() {
  const { t } = useI18n();
  const [ingress, setIngress] = useState<Record<string, boolean>>({});
  const [loadingHarness, setLoadingHarness] = useState<string | null>(null);

  useEffect(() => {
    void fetch("/api/tools/settings")
      .then(async (res) => {
        if (!res.ok) return;
        const data = (await res.json()) as { externalIngress?: Record<string, boolean> };
        if (data.externalIngress) {
          setIngress(data.externalIngress);
        }
      })
      .catch(() => {});
  }, []);

  const handleToggle = async (harnessId: string, nextChecked: boolean) => {
    setLoadingHarness(harnessId);
    setIngress((prev) => ({ ...prev, [harnessId]: nextChecked }));

    try {
      const res = await fetch("/api/tools/settings", {
        method: "PUT",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ externalHarness: harnessId, enabled: nextChecked }),
      });
      if (res.ok) {
        const data = (await res.json()) as { externalIngress?: Record<string, boolean> };
        if (data.externalIngress) {
          setIngress(data.externalIngress);
        }
      }
    } catch {
      // Revert on error
      setIngress((prev) => ({ ...prev, [harnessId]: !nextChecked }));
    } finally {
      setLoadingHarness(null);
    }
  };

  const getStatusText = (status: FormSupportStatus) => {
    if (status === "supported") return t("settings.harnessForms.supported");
    if (status === "in_progress") return t("settings.harnessForms.inProgress");
    return t("settings.harnessForms.unsupported");
  };

  return (
    <div className="settings-general">
      <h2 className="settings-general-title">{t("settings.externalSessions")}</h2>
      <p className="settings-general-description" style={{ marginTop: 6, marginBottom: 20 }}>
        {t("settings.externalSessionsDescription")}
      </p>

      <div className="external-sessions-list">
        {EXTERNAL_HARNESSES.map((harness) => {
          const isEnabled = ingress[harness.id] !== false;
          const isLoading = loadingHarness === harness.id;

          return (
            <div key={harness.id} className={`external-harness-card${isEnabled ? " is-active" : ""}`}>
              <div className="external-harness-card-header">
                <div className="external-harness-brand">
                  <span className="external-harness-icon-wrap">
                    {harness.iconId === "pi" ? (
                      <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" style={{ color: "var(--text-muted)" }}>
                        <polyline points="4 17 10 11 4 5" />
                        <line x1="12" y1="19" x2="20" y2="19" />
                      </svg>
                    ) : (
                      <ProviderIcon id={providerIconId(harness.iconId)} size={20} />
                    )}
                  </span>
                  <div className="external-harness-info">
                    <span className="external-harness-name">{harness.name}</span>
                    <span className="external-harness-vendor">{harness.vendor}</span>
                  </div>
                </div>

                <div className="external-harness-toggle">
                  <ConfigSwitch
                    checked={isEnabled}
                    loading={isLoading}
                    label={`${t("settings.externalSessions")} - ${harness.name}`}
                    onChange={(checked) => void handleToggle(harness.id, checked)}
                  />
                </div>
              </div>

              <div className="external-harness-capsules">
                <FormCapsule
                  formLabel={t("settings.harnessForms.cli")}
                  status={harness.forms.cli}
                  statusLabel={getStatusText(harness.forms.cli)}
                />
                <FormCapsule
                  formLabel={t("settings.harnessForms.desktop")}
                  status={harness.forms.desktop}
                  statusLabel={getStatusText(harness.forms.desktop)}
                />
                <FormCapsule
                  formLabel={t("settings.harnessForms.vscode")}
                  status={harness.forms.vscode}
                  statusLabel={getStatusText(harness.forms.vscode)}
                />
              </div>
            </div>
          );
        })}
      </div>
    </div>
  );
}
