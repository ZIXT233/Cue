import { readFile } from "node:fs/promises";
import { join } from "node:path";
import { sshExec, shellQuote } from "../ssh-workspace";
import type { QueueWorkspace } from "../card-queue";

// Read the existing low-priority defaults before adding a session-only Gemini layer.
export async function inheritedConfig(kind: "gemini" | "opencode", workspace: QueueWorkspace, node: string): Promise<Record<string, unknown>> {
  let text: string;
  if (workspace.kind === "ssh") {
    const script = kind === "opencode" ? 'process.stdout.write(process.env.OPENCODE_CONFIG_CONTENT||"{}");' :
      'const fs=require("node:fs"),p=require("node:path");const f=process.env.GEMINI_CLI_SYSTEM_DEFAULTS_PATH||(process.platform==="darwin"?"/Library/Application Support/GeminiCli/system-defaults.json":"/etc/gemini-cli/system-defaults.json");try{process.stdout.write(fs.readFileSync(f,"utf8"));}catch(e){if(e.code!=="ENOENT")throw e;process.stdout.write("{}");}';
    text = (await sshExec(workspace.sshHost!, [node, "-e", script].map(shellQuote).join(" "))).toString();
  } else if (kind === "opencode") text = process.env.OPENCODE_CONFIG_CONTENT || "{}";
  else {
    const file = process.env.GEMINI_CLI_SYSTEM_DEFAULTS_PATH || (process.platform === "darwin" ? "/Library/Application Support/GeminiCli/system-defaults.json" : process.platform === "win32" ? join(process.env.ProgramData || "C:\\ProgramData", "gemini-cli", "system-defaults.json") : "/etc/gemini-cli/system-defaults.json");
    try { text = await readFile(file, "utf8"); }
    catch (error) { if ((error as NodeJS.ErrnoException).code !== "ENOENT") throw error; text = "{}"; }
  }
  const config = JSON.parse(text);
  if (!config || Array.isArray(config) || typeof config !== "object") throw new Error("无法读取现有 CLI 配置，未覆盖配置");
  return config;
}
