export function isTerminalAbortError(error: unknown) {
  return (error instanceof DOMException && error.name === "AbortError")
    || (error instanceof Error && /abort/i.test(error.message));
}

export async function terminalRequest(path: string, options?: RequestInit): Promise<{ id?: string; cwd?: string; readOnly?: boolean }> {
  const response = await fetch(path, { ...options, signal: options?.signal ?? AbortSignal.timeout(15_000) });
  const data = await response.json().catch(() => {
    throw new Error(`Terminal request failed (HTTP ${response.status}): server returned an empty or invalid JSON response. Check the Cue server log.`);
  });
  if (!data || typeof data !== "object") throw new Error(`Terminal request failed (HTTP ${response.status}): invalid JSON response. Check the Cue server log.`);
  if (!response.ok) throw new Error(data.error ?? `HTTP ${response.status}`);
  return data;
}

export function createTerminalWriter(id: string, onError: (error: Error) => void) {
  let pending = Promise.resolve();
  let stopped = false;
  let bufferedInput: { type: "input"; data: string } | null = null;
  const enqueue = (body: Record<string, unknown> | FormData | (() => Promise<Record<string, unknown>>)) => {
    if (stopped) return;
    pending = pending.then(async () => {
      if (body === bufferedInput) bufferedInput = null;
      const payload = typeof body === "function" ? await body() : body;
      const multipart = payload instanceof FormData;
      await terminalRequest(`/api/terminal/${encodeURIComponent(id)}`, {
        method: "POST",
        headers: multipart ? undefined : { "Content-Type": "application/json" },
        body: multipart ? payload : JSON.stringify(payload),
        ...((multipart || payload.type === "images") ? { signal: AbortSignal.timeout(240_000) } : {}),
      });
    }).catch((error: Error) => {
      // Delivery is ambiguous after a network error. Never replay shell input.
      stopped = true;
      onError(error);
    });
  };
  return {
    write(data: string) {
      if (stopped) return;
      for (const chunk of data.match(/[\s\S]{1,32768}/gu) ?? []) {
        if (bufferedInput && bufferedInput.data.length + chunk.length <= 65536) bufferedInput.data += chunk;
        else {
          bufferedInput = { type: "input", data: chunk };
          enqueue(bufferedInput);
        }
      }
    },
    pasteImages(files: File[], bracketed: boolean) {
      bufferedInput = null;
      enqueue(async () => ({
        type: "images", bracketed,
        images: await Promise.all(files.map(async (file) => {
          const bytes = new Uint8Array(await file.arrayBuffer());
          let binary = "";
          for (const byte of bytes) binary += String.fromCharCode(byte);
          return { type: "image", mimeType: file.type, data: btoa(binary) };
        })),
      }));
    },
    pasteFiles(files: File[], bracketed: boolean) {
      bufferedInput = null;
      const form = new FormData();
      for (const file of files) form.append("files", file, file.name);
      form.append("bracketed", String(bracketed));
      enqueue(form);
    },
    resize(cols: number, rows: number) {
      bufferedInput = null;
      enqueue({ type: "resize", cols, rows });
    },
    stop() { stopped = true; return pending; },
  };
}
