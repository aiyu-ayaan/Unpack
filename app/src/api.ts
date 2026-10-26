import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

export type Step = { label: string; display: string; needs_root: boolean };
export type Plan = {
  id: string; title: string; kind: string; name: string; version: string | null;
  source: string; sha256: string | null; warnings: string[]; advice: string[];
  steps: Step[]; supported: boolean; preview: string | null;
  root_optional: boolean; needs_root: boolean;
};
export type SystemInfo = {
  distro: string; id: string; version: string; family: string; native: string | null;
  arch: string; managers: string[]; runtimes: string[]; helpers: string[];
  missing_helpers: string[]; immutable: boolean; musl: boolean; is_root: boolean;
};
export type Uninstall =
  | { type: "manager"; manager: string; ident: string; user: boolean }
  | { type: "files"; paths: string[] }
  | { type: "traced" }
  | { type: "manual"; note: string };
export type App = {
  id: number; name: string; version: string | null; kind: string; source: string | null;
  sha256: string | null; installed_at: number; status: string; uninstall: Uninstall;
};
export type SysPkg = { name: string; version: string; manager: string };
export type Event = { ts: number; level: string; message: string };
export type RunResult = { ok: boolean; app_id: number | null; error: string | null };
export type LogLine = { plan_id: string; stream: string; line: string };

export const api = {
  system: () => invoke<SystemInfo>("detect_system"),
  analyze: (input: string, allowHttp: boolean) => invoke<Plan>("analyze", { input, allowHttp }),
  execute: (planId: string, asRoot: boolean) => invoke<RunResult>("execute", { planId, asRoot }),
  apps: () => invoke<App[]>("list_apps"),
  history: (appId: number | null = null) => invoke<Event[]>("history", { appId }),
  scan: () => invoke<SysPkg[]>("scan_system"),
  uninstall: (appId: number) => invoke<RunResult>("uninstall_app", { appId }),
  uninstallSystem: (manager: string, ident: string) =>
    invoke<RunResult>("uninstall_system", { manager, ident }),
  onLog: (fn: (l: LogLine) => void) => listen<LogLine>("plan-log", (e) => fn(e.payload)),
};

export const errText = (e: unknown) => (typeof e === "string" ? e : e instanceof Error ? e.message : JSON.stringify(e));
export const when = (ts: number) => new Date(ts * 1000).toLocaleString();
