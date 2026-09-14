export interface RemoteHost {
  id: string;
  name: string;
  hostname: string;
  user?: string;
  port?: number;
  source: "config" | "web";
  visible?: boolean;
  connected?: boolean;
}
