import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

export type Step = { label: string; display: string; needs_root: boolean };
export type Plan = {
  id: string; title: string; kind: string; name: string; version: string | null;
  source: string; fetched: string | null; sha256: string | null; warnings: string[]; advice: string[];
  steps: Step[]; supported: boolean; preview: string | null; sandbox: boolean; can_package: boolean;
  root_optional: boolean; needs_root: boolean;
};
export type SystemInfo = {
  distro: string; id: string; version: string; family: string; native: string | null;
  arch: string; managers: string[]; runtimes: string[]; helpers: string[];
  missing_helpers: string[]; immutable: boolean; layer: string | null; no_fhs_loader: boolean; musl: boolean; is_root: boolean;
};
export type Uninstall =
  | { type: "manager"; manager: string; ident: string; user: boolean }
  | { type: "files"; paths: string[] }
  | { type: "container"; container: string; manager: string; ident: string; apps: string[]; bins: string[] }
  | { type: "traced" }
  | { type: "manual"; note: string };
export type App = {
  id: number; name: string; version: string | null; kind: string; source: string | null;
  sha256: string | null; installed_at: number; status: string; uninstall: Uninstall;
  /** The record that replaced this one in an update. */
  replaced_by: number | null;
};
export type SysPkg = { name: string; version: string; manager: string };
export type Event = { ts: number; level: string; message: string };
export type RunResult = { ok: boolean; app_id: number | null; error: string | null; notes: string[] };
export type Channel = "alpha" | "beta" | "stable";
export type Background = { enabled: boolean; auto_update: boolean };
export type UpdateInfo = { version: string; channel: Channel; self_install: boolean; skipped: string | null };
export type Offer = { version: string; url: string; notes: string | null };
export type Link = "repo" | "issues" | "developer" | "site";
export type LogLine = { plan_id: string; stream: string; line: string };
export type Fetched = { done: number; total: number | null };
export type Upgrade = { manager: string; name: string; current: string; latest: string; user: boolean; app_id: number | null; held: boolean };
export type Way = { type: "store"; source: string; id: string } | { type: "input"; input: string } | { type: "none"; why: string };
export type Imported = { item: { name: string; kind: string; source: string | null }; way: Way };
export type SandboxReport = { code: number | null; timed_out: boolean; files: string[]; output: string[] };
export type Entry = { source: string; id: string; name: string; summary: string; icon: string | null };
export type Remote = { name: string; url: string; user: boolean };
export type Detail = {
  description: string; screenshots: string[]; permissions: string[]; developer: string | null; license: string | null;
  download_size: number | null; installed_size: number | null;
};

export const api = {
  system: () => invoke<SystemInfo>("detect_system"),
  /** `asPackage`: build source into a native package (offered when a plan has `can_package`). */
  analyze: (input: string, allowHttp: boolean, asPackage = false) => invoke<Plan>("analyze", { input, allowHttp, asPackage }),
  storePopular: () => invoke<Entry[]>("store_popular"),
  storeSearch: (query: string) => invoke<Entry[]>("store_search", { query }),
  /** Flathub category name (`Game`, `Graphics`…), with the matching Snap Store section. */
  storeCategory: (category: string) => invoke<Entry[]>("store_category", { category }),
  flatpakRemotes: () => invoke<Remote[]>("flatpak_remotes"),
  /** A reviewable plan that removes Flatpak source `name`. */
  remoteRemovePlan: (name: string) => invoke<Plan>("remote_remove_plan", { name }),
  /** Description, screenshots and permissions of one Store entry. */
  storeDetail: (source: string, id: string) => invoke<Detail>("store_detail", { source, id }),
  /** A reviewable plan for a Store entry; the backend checks the id and keeps the plan. */
  storePlan: (source: string, id: string) => invoke<Plan>("store_plan", { source, id }),
  upgrades: () => invoke<Upgrade[]>("list_upgrades"),
  /** Turn down an update: with `version`, skip only that version; without, never update it. */
  holdUpdate: (manager: string, name: string, version: string | null) => invoke<void>("hold_update", { manager, name, version }),
  unholdUpdate: (manager: string, name: string) => invoke<void>("unhold_update", { manager, name }),
  upgradePlan: (picks: { manager: string; name: string; user: boolean }[]) => invoke<Plan>("upgrade_plan", { picks }),
  /** Analyzes the newer release file of tracked app `appId`; installing it replaces the old one. */
  updateAppPlan: (appId: number) => invoke<Plan>("update_app_plan", { appId }),
  /** Reinstalls the version tracked app `appId` had before Unpack updated it. */
  rollbackPlan: (appId: number) => invoke<Plan>("rollback_plan", { appId }),
  /** Repair plan for a tracked app; unsupported with advice when nothing is wrong. */
  fixPlan: (appId: number) => invoke<Plan>("fix_plan", { appId }),
  /** The file the window was opened with ("Open with Unpack"), once. */
  takeOpened: () => invoke<string | null>("take_opened"),
  /** Another "Open with Unpack" handed a file to this window; take it with `takeOpened`. */
  onOpened: (fn: () => void) => listen("opened", () => fn()),
  fileHandler: () => invoke<boolean>("file_handler"),
  setFileHandler: (on: boolean) => invoke<void>("set_file_handler", { on }),
  /** Saves the installed apps to ~/unpack-apps.json and returns that path. */
  exportApps: () => invoke<string>("export_apps"),
  importApps: (path: string) => invoke<Imported[]>("import_apps", { path }),
  /** Runs a script plan in a bubblewrap sandbox: read-only system, no network, throwaway home. */
  sandboxPreview: (planId: string) => invoke<SandboxReport>("sandbox_preview", { planId }),
  cancelAnalyze: () => invoke<void>("cancel_analyze"),
  onAnalyzeProgress: (fn: (p: Fetched) => void) => listen<Fetched>("analyze-progress", (e) => fn(e.payload)),
  execute: (planId: string, asRoot: boolean) => invoke<RunResult>("execute", { planId, asRoot }),
  apps: () => invoke<App[]>("list_apps"),
  history: (appId: number | null = null) => invoke<Event[]>("history", { appId }),
  scan: () => invoke<SysPkg[]>("scan_system"),
  uninstall: (appId: number) => invoke<RunResult>("uninstall_app", { appId }),
  /** Settings, cache and data folders a removed app left behind. */
  leftovers: (appId: number) => invoke<string[]>("leftovers", { appId }),
  removeLeftovers: (appId: number) => invoke<string[]>("remove_leftovers", { appId }),
  uninstallSystem: (manager: string, ident: string) =>
    invoke<RunResult>("uninstall_system", { manager, ident }),
  backgroundInfo: () => invoke<Background>("background_info"),
  /** Daily check while Unpack is closed; `autoUpdate` also installs updates that need no password. */
  setBackground: (on: boolean, autoUpdate: boolean) => invoke<void>("set_background", { on, autoUpdate }),
  /** Waiting updates, shown on the tray icon. */
  setBadge: (count: number) => invoke<void>("set_badge", { count }),
  updateInfo: () => invoke<UpdateInfo>("update_info"),
  updateCheck: () => invoke<Offer | null>("update_check"),
  /** Resolves with where a package was saved, or null when the app restarts into the new version. */
  updateInstall: () => invoke<string | null>("update_install"),
  updateSetChannel: (channel: Channel) => invoke<void>("update_set_channel", { channel }),
  updateSkip: () => invoke<void>("update_skip"),
  openLink: (which: Link) => invoke<void>("open_link", { which }),
  onUpdateProgress: (fn: (p: number | null) => void) => listen<number | null>("update-progress", (e) => fn(e.payload)),
  onLog: (fn: (l: LogLine) => void) => listen<LogLine>("plan-log", (e) => fn(e.payload)),
};

export const errText = (e: unknown) => (typeof e === "string" ? e : e instanceof Error ? e.message : JSON.stringify(e));
export const when = (ts: number) => new Date(ts * 1000).toLocaleString();
