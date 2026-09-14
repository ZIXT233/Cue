export interface CueDesktop {
  storage?: Pick<Storage, "getItem" | "setItem" | "removeItem">;
  platform?: string;
  writeClipboardText?: (text: string) => Promise<boolean | void> | boolean | void;
  setWindowTheme?: (dark: boolean) => void;
  owner?: string;
  openCard?: (cardId: string) => void | boolean | Promise<boolean | void>;
  focus?: () => void;
  openNotification?: (url: string) => void;
  requestNotifications?: () => Promise<NotificationPermission | boolean | "requested" | "settings-opened">;
  openNotificationSettings?: () => Promise<boolean> | boolean;
}

declare global {
  interface Window {
    topcardDesktop?: CueDesktop;
    cueDesktop?: CueDesktop;
    __CUE_API_BASE__?: string;
  }
}

export function desktopBridge(): CueDesktop | undefined {
  return window.cueDesktop ?? window.topcardDesktop;
}
