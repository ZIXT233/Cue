/** Desktop storage is owned by the main process, independent of the backend port. */
export function persistentStorage(): Pick<Storage, "getItem" | "setItem" | "removeItem"> {
  const desktop = (window as Window & {
    queDesktop?: { storage?: Pick<Storage, "getItem" | "setItem" | "removeItem"> };
  }).queDesktop;
  return desktop?.storage ?? window.localStorage;
}
