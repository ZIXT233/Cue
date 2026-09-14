export function copyText(text: string): Promise<void> {
  const desktop = typeof window === "undefined" ? undefined : (window as Window & { cueDesktop?: { writeClipboardText?: (text: string) => Promise<boolean> } }).cueDesktop;
  if (desktop?.writeClipboardText && text.length <= 1024 * 1024) {
    return Promise.resolve(desktop.writeClipboardText(text)).then((ok) => {
      if (ok === false) throw new Error("Clipboard write failed");
    });
  }
  if (navigator.clipboard?.writeText) {
    return navigator.clipboard.writeText(text);
  }
  try {
    const previousFocus = document.activeElement;
    const ta = document.createElement("textarea");
    ta.value = text;
    ta.style.position = "fixed";
    ta.style.opacity = "0";
    document.body.appendChild(ta);
    ta.select();
    const copied = document.execCommand("copy");
    document.body.removeChild(ta);
    if (previousFocus instanceof HTMLElement) previousFocus.focus();
    if (!copied) return Promise.reject(new Error("Clipboard write failed"));
    return Promise.resolve();
  } catch {
    return Promise.reject();
  }
}
