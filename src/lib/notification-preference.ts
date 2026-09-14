export function notificationEnabledByDefault(
  stored: string | null,
  permission: NotificationPermission,
): boolean {
  return stored !== "false" && permission !== "denied";
}
