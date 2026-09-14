import { useState } from "react";
import { useI18n } from "@/hooks/useI18n";
import { copyText } from "@/lib/clipboard";

export function ErrorDialog({
  title,
  message,
  onDismiss,
  onRetry,
  retryLabel,
  extraAction,
}: {
  title?: string;
  message: string;
  onDismiss: () => void;
  onRetry?: () => void;
  retryLabel?: string;
  extraAction?: { label: string; onClick: () => void };
}) {
  const { t } = useI18n();
  const [copied, setCopied] = useState(false);
  const heading = title || t("error.title");

  const copy = async () => {
    try {
      await copyText(message);
      setCopied(true);
      window.setTimeout(() => setCopied(false), 1600);
    } catch {
      setCopied(false);
    }
  };

  return (
    <div className="cq-overlay cq-error-overlay" role="presentation" onClick={(event) => { if (event.target === event.currentTarget) onDismiss(); }}>
      <section className="cq-dialog cq-error-dialog" role="alertdialog" aria-modal="true" aria-labelledby="cq-error-title" onClick={(event) => event.stopPropagation()}>
        <div className="cq-dialog-heading">
          <strong id="cq-error-title">{heading}</strong>
          <button type="button" aria-label={t("i18n.close")} onClick={onDismiss}>×</button>
        </div>
        <pre className="cq-error-body" tabIndex={0}>{message}</pre>
        <div className="cq-error-actions">
          <button type="button" className="cq-error-copy" onClick={() => void copy()}>{copied ? t("error.copied") : t("error.copy")}</button>
          {onRetry && <button type="button" className="cq-primary" onClick={onRetry}>{retryLabel || t("queue.重试")}</button>}
          {extraAction && <button type="button" onClick={extraAction.onClick}>{extraAction.label}</button>}
          <button type="button" onClick={onDismiss}>{t("error.dismiss")}</button>
        </div>
      </section>
    </div>
  );
}
