export interface RemoteHost {
  id: string;
  name: string;
  hostname: string;
  user?: string;
  port?: number;
  /** IdentityFile from ~/.ssh/config, for hosts that come from the ssh config. */
  identityFile?: string;
  source: "config" | "web";
  visible?: boolean;
  connected?: boolean;
}
