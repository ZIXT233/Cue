import { useCallback, useMemo, useSyncExternalStore } from "react";

function currentUrl(): URL {
  return new URL(window.location.href);
}

function subscribe(onStoreChange: () => void) {
  window.addEventListener("popstate", onStoreChange);
  window.addEventListener("cue:location", onStoreChange);
  return () => {
    window.removeEventListener("popstate", onStoreChange);
    window.removeEventListener("cue:location", onStoreChange);
  };
}

function snapshot() {
  return window.location.href;
}

export function useSearchParams() {
  const href = useSyncExternalStore(subscribe, snapshot, snapshot);
  return useMemo(() => new URL(href).searchParams, [href]);
}

export function useRouter() {
  const replace = useCallback((href: string) => {
    const next = new URL(href, window.location.href);
    window.history.replaceState({}, "", `${next.pathname}${next.search}${next.hash}`);
    window.dispatchEvent(new Event("cue:location"));
  }, []);
  const push = useCallback((href: string) => {
    const next = new URL(href, window.location.href);
    window.history.pushState({}, "", `${next.pathname}${next.search}${next.hash}`);
    window.dispatchEvent(new Event("cue:location"));
  }, []);
  return { replace, push };
}

export function usePathname() {
  const href = useSyncExternalStore(subscribe, snapshot, snapshot);
  return new URL(href).pathname;
}

export { currentUrl };
