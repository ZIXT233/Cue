let apiBase = "";

export function getApiBase() {
  return apiBase;
}

export function setApiBase(base: string) {
  apiBase = base.replace(/\/$/, "");
}

export function apiUrl(path: string) {
  if (/^https?:\/\//.test(path)) return path;
  return `${apiBase}${path}`;
}

export function installApiInterceptor() {
  const originalFetch = window.fetch.bind(window);
  window.fetch = ((input: RequestInfo | URL, init?: RequestInit) => {
    if (typeof input === "string" && input.startsWith("/")) input = apiUrl(input);
    else if (input instanceof URL && input.origin === window.location.origin && input.pathname.startsWith("/api/")) {
      input = new URL(apiUrl(`${input.pathname}${input.search}`));
    } else if (input instanceof Request && input.url.startsWith(window.location.origin) && new URL(input.url).pathname.startsWith("/api/")) {
      const url = new URL(input.url);
      input = new Request(apiUrl(`${url.pathname}${url.search}`), input);
    }
    return originalFetch(input, init);
  }) as typeof window.fetch;

  const OriginalEventSource = window.EventSource;
  window.EventSource = class extends OriginalEventSource {
    constructor(url: string | URL, eventSourceInitDict?: EventSourceInit) {
      const href = typeof url === "string" ? url : url.toString();
      super(href.startsWith("/") ? apiUrl(href) : href, eventSourceInitDict);
    }
  } as typeof EventSource;
}
