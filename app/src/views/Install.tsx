import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { AlertTriangle, Check, CheckCircle2, ChevronRight, Copy, FileDown, Info, Loader2, Lock, Package, Play, Sparkles, X, XCircle } from "lucide-react";
import { api, errText, type LogLine, type Plan } from "../api";
import type { Task } from "../App";
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

export default function Install({ onInstalled, onTask }: { onInstalled: () => void; onTask: (t: Task) => void }) {
  const [text, setText] = useState("");
  const [phase, setPhase] = useState<Phase>("idle");
  const [plan, setPlan] = useState<Plan | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [lines, setLines] = useState<LogLine[]>([]);
  const [asRoot, setAsRoot] = useState(false);
  const [allowHttp, setAllowHttp] = useState(false);
  const [drag, setDrag] = useState(false);
  const [showPreview, setShowPreview] = useState(false);
  const [copied, setCopied] = useState(false);
  const planId = useRef<string | null>(null);

  const analyze = useCallback(async (input: string) => {
    if (!input.trim()) return;
    setPhase("analyzing"); setError(null); setPlan(null); setAsRoot(false); setShowPreview(false);
    try {
      const p = await api.analyze(input.trim(), allowHttp);
      planId.current = p.id; setPlan(p); setPhase("review");
    } catch (e) { setError(errText(e)); setPhase("idle"); }
  }, [allowHttp]);

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
    if (phase === "analyzing") onTask({ label: "analyzing…", progress: null });
    else if (phase === "running" && plan) onTask({ label: waitingRoot ? "waiting for admin password" : `installing ${plan.name}`, progress });
    else onTask(null);
  }, [phase, plan, progress, waitingRoot, onTask]);

  const run = async () => {
    if (!plan) return;
    setLines([]); setPhase("running"); setError(null);
    try {
      const r = await api.execute(plan.id, asRoot);
      if (r.ok) { setPhase("done"); onInstalled(); } else { setError(r.error); setPhase("failed"); }
    } catch (e) { setError(errText(e)); setPhase("failed"); }
  };

  const reset = () => { setPhase("idle"); setPlan(null); setError(null); setText(""); setLines([]); };
  const sudo = error?.match(/^\s+(sudo sh -c .*)$/m)?.[1];
  const copy = (s: string) => { void navigator.clipboard.writeText(s).then(() => { setCopied(true); setTimeout(() => setCopied(false), 1500); }); };

  return (
    <div className="mx-auto flex max-w-3xl flex-col gap-5">
      <PageHeader title="install" sub="Drop a file, paste a link, a curl/wget command or a git address. Unpack figures out the rest." />

      {(phase === "idle" || phase === "analyzing") && (
        <section className={`card enter p-5 transition-[border-color,box-shadow] duration-200 ${drag ? "border-accent! shadow-[0_0_0_3px_color-mix(in_srgb,var(--accent)_25%,transparent)]" : ""}`}>
          <div className={`flex flex-col items-center gap-2 rounded-md border border-dashed py-10 text-center transition-colors duration-200 ${drag ? "border-accent bg-accent/5" : "border-line"}`}>
            <FileDown className={`h-8 w-8 text-accent-ink transition-transform duration-200 ${drag ? "-translate-y-1 scale-110" : ""}`} aria-hidden />
            <div className="font-mono text-base font-semibold">{drag ? "release to analyze" : "drag a file here"}</div>
            <div className="text-sm text-dim">or paste below</div>
          </div>
          <form className="mt-4 flex gap-2" onSubmit={(e) => { e.preventDefault(); void analyze(text); }}>
            <label className="relative flex-1">
              <span className="sr-only">File path, URL, git address or curl/wget command</span>
              <span className="pointer-events-none absolute left-3 top-1/2 -translate-y-1/2 font-mono text-sm text-accent-ink" aria-hidden>$</span>
              <input className="input pl-7!" placeholder="/path/to/app.deb · https://… · curl -fsSL https://… | sh"
                value={text} onChange={(e) => setText(e.target.value)} disabled={phase === "analyzing"} spellCheck={false} autoFocus />
            </label>
            <button className="btn btn-primary shrink-0" disabled={phase === "analyzing" || !text.trim()}>
              {phase === "analyzing" ? <Loader2 className="h-4 w-4 animate-spin" aria-hidden /> : <Sparkles className="h-4 w-4" aria-hidden />} analyze
            </button>
          </form>
          {phase === "analyzing" && <div className="mt-3"><Progress value={null} label="Analyzing" /></div>}
          <label className="mt-3 flex cursor-pointer items-center gap-2 text-xs text-dim">
            <input type="checkbox" className="accent-[var(--accent)]" checked={allowHttp} onChange={(e) => setAllowHttp(e.target.checked)} /> Allow unencrypted http:// links (not recommended)
          </label>
          {error && <div className="mt-3"><Banner tone="bad" icon={<XCircle className="h-4 w-4" />}>{error}</Banner></div>}
          <div className="mt-5 flex flex-wrap gap-1.5">{FORMATS.map((f) => <span key={f} className="chip">{f}</span>)}</div>
        </section>
      )}

      {plan && phase !== "idle" && phase !== "analyzing" && (
        <section className="card enter flex flex-col gap-4 p-5">
          <div className="flex items-start gap-4">
            <div className="grid h-11 w-11 shrink-0 place-items-center rounded-md border border-accent/40 bg-accent/10"><Package className="h-5 w-5 text-accent-ink" aria-hidden /></div>
            <div className="min-w-0 flex-1">
              <h2 className="truncate font-mono text-lg font-semibold">{plan.name}{plan.version && <span className="ml-2 text-sm font-normal text-dim">{plan.version}</span>}</h2>
              <div className="mt-1 flex flex-wrap gap-1.5"><span className="chip">{plan.kind}</span>{plan.needs_root && <span className="chip chip-warn"><Lock className="h-3 w-3" aria-hidden /> admin password</span>}</div>
              <div className="mt-1 truncate text-xs text-dim" title={plan.source}>{plan.source}</div>
              {plan.sha256 && <div className="truncate font-mono text-[10px] text-dim" title={plan.sha256}>sha256 {plan.sha256}</div>}
            </div>
            {phase === "review" && <button className="btn" onClick={reset} aria-label="Cancel"><X className="h-4 w-4" /></button>}
          </div>

          {plan.warnings.map((w, i) => <Banner key={i} tone={plan.supported ? "warn" : "bad"} icon={plan.supported ? <AlertTriangle className="h-4 w-4" /> : <XCircle className="h-4 w-4" />}>{w}</Banner>)}
          {plan.advice.map((a, i) => <Banner key={i} tone="info" icon={<Info className="h-4 w-4" />}>{a}</Banner>)}

          {phase !== "review" && (
            <div className="flex items-center gap-3">
              <div className="flex-1"><Progress value={phase === "failed" ? Math.max(0, active) / Math.max(1, total) : progress} label="Install progress" /></div>
              <span className="w-10 text-right font-mono text-xs text-dim">{progress === null ? "" : `${Math.round(progress * 100)}%`}</span>
            </div>
          )}

          {plan.steps.length > 0 && (
            <ol className="stagger flex flex-col gap-1.5">
              {plan.steps.map((s, i) => {
                const st = phase === "done" || (phase !== "review" && i < active) ? "done" : phase === "running" && i === active ? "run" : phase === "failed" && i === active ? "fail" : "wait";
                return (
                  <li key={i} className={`rounded-md border p-3 transition-colors duration-300 ${st === "run" ? "border-accent/60 bg-accent/5" : st === "fail" ? "border-bad/60" : "border-line bg-raised"}`}>
                    <div className="flex items-center gap-2 text-sm font-medium">
                      <span className="grid h-5 w-5 shrink-0 place-items-center" aria-label={st}>
                        {st === "done" ? <Check className="pop h-4 w-4 text-accent-ink" /> : st === "run" ? <Loader2 className="h-4 w-4 animate-spin text-accent-ink" />
                          : st === "fail" ? <XCircle className="pop h-4 w-4 text-bad" /> : <span className="font-mono text-[11px] text-dim">{i + 1}</span>}
                      </span>
                      {s.label}{s.needs_root && <Lock className="h-3 w-3 text-warn" aria-label="needs admin" />}
                    </div>
                    <code className="mt-1 block truncate pl-7 font-mono text-xs text-dim" title={s.display}><span className="text-accent-ink">$</span> {s.display}</code>
                  </li>
                );
              })}
            </ol>
          )}

          {plan.preview && (
            <div>
              <button className="flex cursor-pointer items-center gap-1 font-mono text-xs text-dim hover:text-ink" onClick={() => setShowPreview((v) => !v)} aria-expanded={showPreview}>
                <ChevronRight className={`h-4 w-4 transition-transform duration-200 ${showPreview ? "rotate-90" : ""}`} aria-hidden /> {showPreview ? "hide" : "show"} contents
              </button>
              {showPreview && <pre className="enter mt-2 max-h-72 overflow-auto rounded-md border border-line bg-term p-3 font-mono text-xs">{plan.preview}</pre>}
            </div>
          )}

          {phase === "review" && plan.root_optional && (
            <label className="flex cursor-pointer items-center gap-2 text-sm"><input type="checkbox" className="accent-[var(--accent)]" checked={asRoot} onChange={(e) => setAsRoot(e.target.checked)} /> Run as administrator (only if it fails without)</label>
          )}

          {waitingRoot && <Banner tone="warn" icon={<Lock className="h-4 w-4" />}>Enter your password in the system prompt to continue. If no window appears, see the message after it fails.</Banner>}
          {phase !== "review" && <Log lines={lines} running={phase === "running"} />}
          {phase === "done" && <Banner tone="good" icon={<CheckCircle2 className="h-4 w-4" />}>Installed. You can find it in your Library.</Banner>}
          {phase === "failed" && <Banner tone="bad" icon={<XCircle className="h-4 w-4" />}>{error ?? "Install failed"}</Banner>}
          {phase === "failed" && sudo && <button className="btn self-start" onClick={() => copy(sudo)}><Copy className="h-4 w-4" aria-hidden /> {copied ? "copied" : "copy terminal command"}</button>}

          <div className="flex justify-end gap-2">
            {phase === "review" && <button className="btn btn-primary" disabled={!plan.supported} onClick={run}><Play className="h-4 w-4" aria-hidden /> install</button>}
            {phase === "running" && <button className="btn" disabled><Loader2 className="h-4 w-4 animate-spin" aria-hidden /> working…</button>}
            {(phase === "done" || phase === "failed") && <button className="btn" onClick={reset}>install something else</button>}
          </div>
        </section>
      )}
    </div>
  );
}
