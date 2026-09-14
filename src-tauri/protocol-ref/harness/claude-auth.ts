import { mkdir, readFile, rename, writeFile } from "node:fs/promises";
import { dirname, join } from "node:path";
import { dataDir } from "../card-queue-live.ts";

export interface ClaudeHarnessAuth {
  apiKey?: string;
  baseUrl?: string;
}

export function claudeAuthPath(root = dataDir()): string {
  return join(root, "claude-auth.json");
}

export async function readClaudeHarnessAuth(root = dataDir()): Promise<ClaudeHarnessAuth> {
  try {
    const parsed: unknown = JSON.parse(await readFile(claudeAuthPath(root), "utf8"));
    if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) return {};
    const record = parsed as Record<string, unknown>;
    return {
      ...(typeof record.apiKey === "string" && record.apiKey.trim() ? { apiKey: record.apiKey.trim() } : {}),
      ...(typeof record.baseUrl === "string" && record.baseUrl.trim() ? { baseUrl: normalizeClaudeBaseUrl(record.baseUrl) } : {}),
    };
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code === "ENOENT") return {};
    throw error;
  }
}

export function normalizeClaudeBaseUrl(value: string): string {
  const trimmed = value.trim().replace(/\/+$/, "");
  if (!trimmed) return "";
  const url = new URL(trimmed);
  if (!["http:", "https:"].includes(url.protocol)) throw new Error("Base URL must be http or https");
  return url.href.replace(/\/+$/, "");
}

export async function writeClaudeHarnessAuth(
  next: { apiKey?: string | null; baseUrl?: string | null },
  root = dataDir(),
): Promise<ClaudeHarnessAuth> {
  const current = await readClaudeHarnessAuth(root);
  const apiKey = next.apiKey === undefined ? current.apiKey : (next.apiKey?.trim() || undefined);
  const baseUrl = next.baseUrl === undefined
    ? current.baseUrl
    : (next.baseUrl?.trim() ? normalizeClaudeBaseUrl(next.baseUrl) : undefined);
  const auth: ClaudeHarnessAuth = { ...(apiKey ? { apiKey } : {}), ...(baseUrl ? { baseUrl } : {}) };
  const path = claudeAuthPath(root);
  await mkdir(dirname(path), { recursive: true, mode: 0o700 });
  const temporary = `${path}.${process.pid}.tmp`;
  await writeFile(temporary, JSON.stringify(auth), { mode: 0o600 });
  await rename(temporary, path);
  return auth;
}

/** Env Claude Code reads for a console API key / Anthropic-compatible relay. */
export function claudeHarnessEnv(auth: ClaudeHarnessAuth): Record<string, string> {
  const env: Record<string, string> = {};
  if (auth.apiKey) env.ANTHROPIC_API_KEY = auth.apiKey;
  if (auth.baseUrl) {
    env.ANTHROPIC_BASE_URL = auth.baseUrl;
    env.CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC = "1";
  }
  return env;
}
