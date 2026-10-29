import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { AlertTriangle, Box, Check, CheckCircle2, ChevronRight, Copy, FileDown, Info, Loader2, Lock, Package, Play, Sparkles, X, XCircle } from "lucide-react";
import { api, errText, type Fetched, type LogLine, type Plan, type SandboxReport } from "../api";
import type { Incoming, Task } from "../App";
import { Log } from "../components/Log";
import { Banner, PageHeader, Progress } from "../components/ui";

type Phase = "idle" | "analyzing" | "review" | "running" | "done" | "failed";

const FORMATS = [".deb", ".rpm", ".pkg.tar.zst", ".apk", ".snap", ".flatpak", ".AppImage", ".run", ".sh", ".tar.gz", ".zip", ".7z", ".dsc", "git", "curl / wget"];

/** Index of the step now running, from the runner's `== label ==` markers. */
function activeStep(plan: Plan, lines: LogLine[]): number {
  let at = -1;
  for (const l of lines) {
    const m = l.stream === "info" && /^== (.*) ==$/.exec(l.line);
    if (!m) continue;
    const next = plan.steps.findIndex((s, i) => i > at && s.label === m[1]);
    if (next !== -1) at = next;
  }
  return at;
}

const mb = (b: number) => `${(b / 1048576).toFixed(1)} MB`;

/** Plans that change something already there: page title, button, and the message when done. */
const VERB: Record<string, [string, string, string]> = {
  update: ["Update", "Update", "Done."],
  fix: ["Fix an app", "Fix", "Done."],
  "flatpak-remote": ["Remove a Flatpak source", "Remove", "Removed."],
};

export default function Install({ onInstalled, onTask, incoming, onTaken }: { onInstalled: () => void; onTask: (t: Task) => void; incoming: Incoming | null; onTaken: () => void }) {
  const [text, setText] = useState("");
  const [phase, setPhase] = useState<Phase>("idle");
  const [plan, setPlan] = useState<Plan | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [notes, setNotes] = useState<string[]>([]);
  const [lines, setLines] = useState<LogLine[]>([]);
  const [asRoot, setAsRoot] = useState(false);
  const [allowHttp, setAllowHttp] = useState(false);
  const [drag, setDrag] = useState(false);
  const [showPreview, setShowPreview] = useState(false);
  const [copied, setCopied] = useState(false);
  const [fetched, setFetched] = useState<Fetched | null>(null);
  const [trial, setTrial] = useState<SandboxReport | "running" | string | null>(null);
  const planId = useRef<string | null>(null);

  const analyze = useCallback(async (input: string, asPackage = false) => {
    if (!input.trim()) return;
    setPhase("analyzing"); setError(null); setNotes([]); setPlan(null); setAsRoot(false); setShowPreview(false); setFetched(null); setTrial(null);
    try {
      const p = await api.analyze(input.trim(), allowHttp, asPackage);
      planId.current = p.id; setPlan(p); setPhase("review");
    } catch (e) { const m = errText(e); setError(m === "Cancelled" ? null : m); setPhase("idle"); }
  }, [allowHttp]);

  // A plan or input handed over by another view replaces whatever is here, unless an install is running.
  useEffect(() => {
    if (!incoming || phase === "running" || phase === "analyzing") return;
    onTaken();
    if ("plan" in incoming) {
      setError(null); setNotes([]); setLines([]); setAsRoot(false); setShowPreview(false);
      setTrial(null); planId.current = incoming.plan.id; setPlan(incoming.plan); setPhase("review");
    } else { setText(incoming.input); void analyze(incoming.input); }
  }, [incoming, phase, analyze, onTaken]);

  useEffect(() => {
    const un = api.onAnalyzeProgress(setFetched);
    return () => { void un.then((f) => f()); };
  }, []);

  useEffect(() => {
    const un = getCurrentWebview().onDragDropEvent((e) => {
      const t = e.payload.type;
      if (t === "enter" || t === "over") setDrag(true);
      else if (t === "leave") setDrag(false);
      else if (t === "drop") {
        setDrag(false);
        const p = e.payload.paths[0];
        if (p) { setText(p); void analyze(p); }
      }
    });
    return () => { void un.then((f) => f()); };
  }, [analyze]);

  useEffect(() => {
    const un = api.onLog((l) => { if (l.plan_id === planId.current) setLines((x) => [...x, l]); });
    return () => { void un.then((f) => f()); };
  }, []);

  const active = useMemo(() => (plan ? activeStep(plan, lines) : -1), [plan, lines]);
  const total = plan?.steps.length ?? 0;
  const progress = phase === "done" ? 1 : total ? Math.max(0, active + 0.5) / total : null;
  const waitingRoot = phase === "running" && lines.at(-1)?.line === "Waiting for the admin password…";

  useEffect(() => {
    if (phase === "analyzing") onTask({ label: fetched?.done ? "Downloading…" : "Checking…", progress: fetched?.total ? fetched.done / fetched.total : null });
    else if (phase === "running" && plan) onTask({ label: waitingRoot ? "Waiting for admin password" : plan.title, progress });
    else onTask(null);
  }, [phase, plan, progress, waitingRoot, fetched, onTask]);

  const run = async () => {
    if (!plan) return;
    setLines([]); setPhase("running"); setError(null); setNotes([]);
    try {
      const r = await api.execute(plan.id, asRoot);
      if (r.ok) { setNotes(r.notes); setPhase("done"); onInstalled(); } else { setError(r.error); setPhase("failed"); }
    } catch (e) { setError(errText(e)); setPhase("failed"); }
  };

  const tryIt = async () => {
    if (!plan) return;
    setTrial("running");
    try { setTrial(await api.sandboxPreview(plan.id)); } catch (e) { setTrial(errText(e)); }
  };

  const reset = () => { setTrial(null); setPhase("idle"); setPlan(null); setError(null); setText(""); setLines([]); };
  const sudo = error?.match(/^\s+(sudo sh -c .*)$/m)?.[1];
  const copy = (s: string) => { void navigator.clipboard.writeText(s).then(() => { setCopied(true); setTimeout(() => setCopied(false), 1500); }); };

  return (
    <div className="mx-auto flex max-w-3xl flex-col gap-5">
      {plan && phase !== "idle" && phase !== "analyzing"
        ? <PageHeader title={VERB[plan.kind]?.[0] ?? "Install an app"} sub="Review what will happen, then go ahead." />
        : <PageHeader title="Install an app" sub="Drop a file, or paste a link, download command or git address. Unpack works out the rest." />}

      {(phase === "idle" || phase === "analyzing") && (
        <section className={"card enter p-5"}>
          <div className={`flex flex-col items-center gap-2 rounded-md border border-dashed py-12 text-center transition-colors duration-150 ${drag ? "border-info bg-info/10" : "border-line"}`}>
            <FileDown className={`h-9 w-9 transition-transform duration-150 ${drag ? "-translate-y-1 text-info" : "text-dim"}`} strokeWidth={1.25} aria-hidden />
            <div className="text-sm font-medium text-strong">{drag ? "Release to check it" : "Drag a file here"}</div>
            <div className="text-dim">or paste below</div>
          </div>
          <form className="mt-4 flex gap-2" onSubmit={(e) => { e.preventDefault(); void analyze(text); }}>
            <label className="relative flex-1">
              <span className="sr-only">File path, URL, git address or curl/wget command</span>
              <input className="input" placeholder="e.g. ~/Downloads/app.deb, https://example.com/app.AppImage, or curl -fsSL … | sh"
                value={text} onChange={(e) => setText(e.target.value)} disabled={phase === "analyzing"} spellCheck={false} autoFocus />
            </label>
            <button className="btn btn-primary shrink-0" disabled={phase === "analyzing" || !text.trim()}>
              {phase === "analyzing" ? <Loader2 className="h-4 w-4 animate-spin" aria-hidden /> : <Sparkles className="h-4 w-4" aria-hidden />} Check
            </button>
          </form>
          {phase === "analyzing" && (
            <div className="mt-3 flex items-center gap-3">
              <div className="flex-1"><Progress value={fetched?.total ? fetched.done / fetched.total : null} label={fetched?.done ? "Downloading" : "Analyzing"} /></div>
              {!!fetched?.done && <span className="text-xs tabular-nums text-dim">{mb(fetched.done)}{fetched.total ? ` of ${mb(fetched.total)}` : ""}</span>}
              <button type="button" className="btn btn-ghost" onClick={() => void api.cancelAnalyze()}><X className="h-4 w-4" aria-hidden /> Cancel</button>
            </div>
          )}
          <label className="mt-3 flex cursor-pointer items-center gap-2 text-xs text-dim">
            <input type="checkbox" className="accent-[var(--accent)]" checked={allowHttp} onChange={(e) => setAllowHttp(e.target.checked)} /> Allow unencrypted http:// links (not recommended)
          </label>
          {error && <div className="mt-3"><Banner tone="bad" icon={<XCircle className="h-4 w-4" />}>{error}</Banner></div>}
          <div className="mt-5 flex flex-wrap items-center gap-1.5"><span className="mr-1 text-xs text-dim">Works with</span>{FORMATS.map((f) => <span key={f} className="chip">{f}</span>)}</div>
        </section>
      )}

      {plan && phase !== "idle" && phase !== "analyzing" && (
        <section className="card enter flex flex-col gap-4 p-5">
          <div className="flex items-start gap-4">
            <div className="grid h-11 w-11 shrink-0 place-items-center rounded-md bg-raised"><Package className="h-5 w-5 text-accent-ink" strokeWidth={1.5} aria-hidden /></div>
            <div className="min-w-0 flex-1">
              <h2 className="truncate text-base font-semibold text-strong">{plan.name}{plan.version && <span className="ml-2 text-sm font-normal text-dim">{plan.version}</span>}</h2>
              <div className="mt-1 flex flex-wrap gap-1.5"><span className="chip">{plan.kind}</span>{plan.needs_root && <span className="chip chip-warn"><Lock className="h-3 w-3" aria-hidden /> Needs admin password</span>}</div>
              <div className="mt-1 truncate text-xs text-dim" title={plan.source}>{plan.source}</div>
              {plan.fetched && <div className="truncate text-[11px] text-dim" title={plan.fetched}>Downloaded from {plan.fetched}</div>}
              {plan.sha256 && <div className="truncate text-[11px] text-dim" title={plan.sha256}>SHA-256 {plan.sha256.slice(0, 16)}…</div>}
            </div>
            {phase === "review" && <button className="btn btn-ghost px-1.5!" onClick={reset} aria-label="Cancel" title="Cancel"><X className="h-4 w-4" /></button>}
          </div>

          {plan.warnings.map((w, i) => <Banner key={i} tone={plan.supported ? "warn" : "bad"} icon={plan.supported ? <AlertTriangle className="h-4 w-4" /> : <XCircle className="h-4 w-4" />}>{w}</Banner>)}
          {plan.advice.map((a, i) => <Banner key={i} tone="info" icon={<Info className="h-4 w-4" />}>{a}</Banner>)}

          {phase !== "review" && (
            <div className="flex items-center gap-3">
              <div className="flex-1"><Progress value={phase === "failed" ? Math.max(0, active) / Math.max(1, total) : progress} label="Install progress" /></div>
              <span className="w-10 text-right text-xs tabular-nums text-dim">{progress === null ? "" : `${Math.round(progress * 100)}%`}</span>
            </div>
          )}

          {plan.steps.length > 0 && (
            <ol className="stagger flex flex-col overflow-hidden rounded-md border border-line">
              {plan.steps.map((s, i) => {
                const st = phase === "done" || (phase !== "review" && i < active) ? "done" : phase === "running" && i === active ? "run" : phase === "failed" && i === active ? "fail" : "wait";
                return (
                  <li key={i} className={`border-b border-line px-3 py-2 transition-colors duration-200 last:border-0 ${st === "run" ? "bg-info/10" : st === "fail" ? "bg-bad/10" : ""}`}>
                    <div className="flex items-center gap-2 text-strong">
                      <span className="grid h-5 w-5 shrink-0 place-items-center" aria-label={st}>
                        {st === "done" ? <Check className="pop h-4 w-4 text-accent-ink" /> : st === "run" ? <Loader2 className="h-4 w-4 animate-spin text-info" />
                          : st === "fail" ? <XCircle className="pop h-4 w-4 text-bad" /> : <span className="text-[11px] text-dim">{i + 1}</span>}
                      </span>
                      {s.label}{s.needs_root && <Lock className="h-3 w-3 text-warn" aria-label="needs admin" />}
                    </div>
                    <code className="block truncate pl-7 font-mono text-[11px] text-dim" title={s.display}>{s.display}</code>
                  </li>
                );
              })}
            </ol>
          )}

          {plan.preview && (
            <div>
              <button className="flex cursor-pointer items-center gap-1 text-dim hover:text-ink" onClick={() => setShowPreview((v) => !v)} aria-expanded={showPreview}>
                <ChevronRight className={`h-4 w-4 transition-transform duration-200 ${showPreview ? "rotate-90" : ""}`} aria-hidden /> {showPreview ? "Hide" : "Show"} contents
              </button>
              {showPreview && <pre className="enter mt-2 max-h-72 overflow-auto rounded-md border border-line bg-term p-3 font-mono text-xs">{plan.preview}</pre>}
            </div>
          )}

          {phase === "review" && plan.sandbox && (
            <div className="flex flex-col gap-2">
              <button className="btn self-start" disabled={trial === "running"} onClick={() => void tryIt()} title="Runs it with the system read-only, no network and an empty home folder">
                {trial === "running" ? <Loader2 className="h-4 w-4 animate-spin" aria-hidden /> : <Box className="h-4 w-4" aria-hidden />} Try it in a sandbox first</button>
              {typeof trial === "string" && trial !== "running" && <Banner tone="bad" icon={<XCircle className="h-4 w-4" />}>{trial}</Banner>}
              {trial && typeof trial === "object" && (
                <Banner tone={trial.code === 0 ? "info" : "warn"} icon={<Box className="h-4 w-4" />}>
                  <div>In the sandbox it {trial.timed_out ? "was stopped after 2 minutes" : trial.code === 0 ? "finished" : `failed (exit ${trial.code ?? "signal"})`}. The network was off and the system read-only, so a real run can differ.</div>
                  <div className="mt-1">{trial.files.length ? `It created ${trial.files.length} file${trial.files.length > 1 ? "s" : ""} in your home folder:` : "It created no files in your home folder."}</div>
                  {trial.files.length > 0 && <ul className="my-1 max-h-32 overflow-auto font-mono text-xs">{trial.files.map((f) => <li key={f}>{f}</li>)}</ul>}
                  {trial.output.length > 0 && <pre className="mt-1 max-h-40 overflow-auto rounded border border-line bg-term p-2 font-mono text-[11px]">{trial.output.join("\n")}</pre>}
                </Banner>
              )}
            </div>
          )}

          {phase === "review" && plan.can_package && text.trim() && (
            <Banner tone="info" icon={<Package className="h-4 w-4" />}>
              <div>This builds from source into your home folder (~/.local). Unpack can instead make a system package of it, so your package manager knows its files and can remove it cleanly.</div>
              <button className="btn mt-2" onClick={() => void analyze(text, true)}>Build a system package instead</button>
            </Banner>
          )}

          {phase === "review" && plan.root_optional && (
            <label className="flex cursor-pointer items-center gap-2 text-sm"><input type="checkbox" className="accent-[var(--accent)]" checked={asRoot} onChange={(e) => setAsRoot(e.target.checked)} /> Run as administrator (only if it fails without)</label>
          )}

          {waitingRoot && <Banner tone="warn" icon={<Lock className="h-4 w-4" />}>Enter your password in the system prompt to continue. If no window appears, see the message after it fails.</Banner>}
          {phase !== "review" && <Log lines={lines} running={phase === "running"} />}
          {phase === "done" && <Banner tone="good" icon={<CheckCircle2 className="h-4 w-4" />}>{VERB[plan.kind]?.[2] ?? "Installed. You can find it in your Library."}</Banner>}
          {phase === "done" && notes.map((n, i) => <Banner key={i} tone="warn" icon={<AlertTriangle className="h-4 w-4" />}>{n}</Banner>)}
          {phase === "failed" && <Banner tone="bad" icon={<XCircle className="h-4 w-4" />}>{error ?? "Install failed"}</Banner>}
          {phase === "failed" && sudo && <button className="btn self-start" onClick={() => copy(sudo)}><Copy className="h-4 w-4" aria-hidden /> {copied ? "Copied" : "Copy terminal command"}</button>}

          <div className="flex justify-end gap-2">
            {phase === "review" && <button className="btn btn-primary" disabled={!plan.supported} onClick={run}><Play className="h-4 w-4" aria-hidden /> {VERB[plan.kind]?.[1] ?? "Install"}</button>}
            {phase === "running" && <button className="btn" disabled><Loader2 className="h-4 w-4 animate-spin" aria-hidden /> Working…</button>}
            {(phase === "done" || phase === "failed") && <button className="btn" onClick={reset}>Done</button>}
          </div>
        </section>
      )}
    </div>
  );
}
