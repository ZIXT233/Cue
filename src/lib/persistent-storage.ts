/** Desktop storage is owned by the main process, independent of the backend port. */
export function persistentStorage(): Pick<Storage, "getItem" | "setItem" | "removeItem"> {
  const desktop = (window as Window & {
    cueDesktop?: { storage?: Pick<Storage, "getItem" | "setItem" | "removeItem"> };
  }).cueDesktop;
  return desktop?.storage ?? window.localStorage;
}
