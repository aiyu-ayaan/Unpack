import { useCallback, useEffect, useRef, useState } from "react";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { AlertTriangle, CheckCircle2, ChevronDown, FileDown, Info, Loader2, Lock, Package, Play, Sparkles, X, XCircle } from "lucide-react";
import { api, errText, type LogLine, type Plan } from "../api";
import { Log } from "../components/Log";

type Phase = "idle" | "analyzing" | "review" | "running" | "done" | "failed";

const FORMATS = [".deb", ".rpm", ".pkg.tar.zst", ".apk", ".snap", ".flatpak", ".AppImage", ".run", ".sh", ".tar.gz", ".zip", ".7z", ".dsc", "git", "curl / wget"];

export default function Install({ onInstalled }: { onInstalled: () => void }) {
  const [text, setText] = useState("");
  const [phase, setPhase] = useState<Phase>("idle");
  const [plan, setPlan] = useState<Plan | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [lines, setLines] = useState<LogLine[]>([]);
  const [asRoot, setAsRoot] = useState(false);
  const [allowHttp, setAllowHttp] = useState(false);
  const [drag, setDrag] = useState(false);
  const [showPreview, setShowPreview] = useState(false);
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

  const run = async () => {
    if (!plan) return;
    setLines([]); setPhase("running");
    try {
      const r = await api.execute(plan.id, asRoot);
      if (r.ok) { setPhase("done"); onInstalled(); } else { setError(r.error); setPhase("failed"); }
    } catch (e) { setError(errText(e)); setPhase("failed"); }
  };

  const reset = () => { setPhase("idle"); setPlan(null); setError(null); setText(""); setLines([]); };

  return (
    <div className="mx-auto flex max-w-3xl flex-col gap-5">
      <header>
        <h1 className="text-3xl font-bold tracking-tight">Install anything</h1>
        <p className="mt-1 text-dim">Drop a file, paste a link, a <code>curl</code>/<code>wget</code> command or a git address. Unpack figures out the rest.</p>
      </header>

      {(phase === "idle" || phase === "analyzing") && (
        <section className={`glass p-6 transition ${drag ? "border-brand! shadow-[0_0_0_4px_#7c5cff33]" : ""}`}>
          <div className="flex flex-col items-center gap-3 rounded-2xl border-2 border-dashed border-line py-10 text-center">
            <div className="grid h-14 w-14 place-items-center rounded-2xl bg-gradient-to-br from-brand to-brand2 shadow-lg">
              <FileDown className="h-7 w-7 text-white" />
            </div>
            <div className="text-lg font-semibold">{drag ? "Drop it here" : "Drag a file here"}</div>
            <div className="text-sm text-dim">or paste below</div>
          </div>
          <form className="mt-4 flex gap-2" onSubmit={(e) => { e.preventDefault(); void analyze(text); }}>
            <input className="input" placeholder="/path/to/app.deb · https://… · curl -fsSL https://… | sh · https://github.com/owner/repo"
              value={text} onChange={(e) => setText(e.target.value)} disabled={phase === "analyzing"} spellCheck={false} />
            <button className="btn btn-primary shrink-0" disabled={phase === "analyzing" || !text.trim()}>
              {phase === "analyzing" ? <Loader2 className="h-4 w-4 animate-spin" /> : <Sparkles className="h-4 w-4" />} Analyze
            </button>
          </form>
          <label className="mt-3 flex items-center gap-2 text-xs text-dim">
            <input type="checkbox" checked={allowHttp} onChange={(e) => setAllowHttp(e.target.checked)} /> Allow unencrypted http:// links (not recommended)
          </label>
          {error && <Banner tone="bad" icon={<XCircle className="h-4 w-4" />}>{error}</Banner>}
          <div className="mt-5 flex flex-wrap gap-1.5">{FORMATS.map((f) => <span key={f} className="chip">{f}</span>)}</div>
        </section>
      )}

      {plan && (phase === "review" || phase === "running" || phase === "done" || phase === "failed") && (
        <section className="glass flex flex-col gap-4 p-6">
          <div className="flex items-start gap-4">
            <div className="grid h-12 w-12 shrink-0 place-items-center rounded-xl bg-gradient-to-br from-brand/80 to-brand2/80"><Package className="h-6 w-6 text-white" /></div>
            <div className="min-w-0 flex-1">
              <h2 className="truncate text-xl font-semibold">{plan.name}{plan.version && <span className="ml-2 text-sm font-normal text-dim">{plan.version}</span>}</h2>
              <div className="mt-1 flex flex-wrap gap-1.5"><span className="chip">{plan.kind}</span>{plan.needs_root && <span className="chip chip-warn"><Lock className="h-3 w-3" /> admin password</span>}</div>
              <div className="mt-1 truncate text-xs text-dim" title={plan.source}>{plan.source}</div>
              {plan.sha256 && <div className="truncate font-mono text-[10px] text-dim" title={plan.sha256}>sha256 {plan.sha256}</div>}
            </div>
            {phase === "review" && <button className="btn" onClick={reset} aria-label="Cancel"><X className="h-4 w-4" /></button>}
          </div>

          {plan.warnings.map((w, i) => <Banner key={i} tone={plan.supported ? "warn" : "bad"} icon={plan.supported ? <AlertTriangle className="h-4 w-4" /> : <XCircle className="h-4 w-4" />}>{w}</Banner>)}
          {plan.advice.map((a, i) => <Banner key={i} tone="info" icon={<Info className="h-4 w-4" />}>{a}</Banner>)}

          {plan.steps.length > 0 && (
            <ol className="flex flex-col gap-2">
              {plan.steps.map((s, i) => (
                <li key={i} className="rounded-xl border border-line bg-[#0e0e1b] p-3">
                  <div className="flex items-center gap-2 text-sm font-medium"><span className="grid h-5 w-5 place-items-center rounded-full bg-line text-[10px]">{i + 1}</span>{s.label}{s.needs_root && <Lock className="h-3 w-3 text-amber-300" />}</div>
                  <code className="mt-1 block truncate text-xs text-dim" title={s.display}>{s.display}</code>
                </li>
              ))}
            </ol>
          )}

          {plan.preview && (
            <div>
              <button className="flex items-center gap-1 text-sm text-dim hover:text-ink" onClick={() => setShowPreview((v) => !v)}>
                <ChevronDown className={`h-4 w-4 transition ${showPreview ? "rotate-180" : ""}`} /> {showPreview ? "Hide" : "Show"} contents
              </button>
              {showPreview && <pre className="mt-2 max-h-72 overflow-auto rounded-xl border border-line bg-[#07070f] p-3 font-mono text-xs">{plan.preview}</pre>}
            </div>
          )}

          {phase === "review" && plan.root_optional && (
            <label className="flex items-center gap-2 text-sm"><input type="checkbox" checked={asRoot} onChange={(e) => setAsRoot(e.target.checked)} /> Run as administrator (only if it fails without)</label>
          )}

          {(phase === "running" || phase === "done" || phase === "failed") && <Log lines={lines} />}
          {phase === "done" && <Banner tone="good" icon={<CheckCircle2 className="h-4 w-4" />}>Installed. You can find it in your Library.</Banner>}
          {phase === "failed" && <Banner tone="bad" icon={<XCircle className="h-4 w-4" />}>{error ?? "Install failed"}</Banner>}

          <div className="flex justify-end gap-2">
            {phase === "review" && <button className="btn btn-primary" disabled={!plan.supported} onClick={run}><Play className="h-4 w-4" /> Install</button>}
            {phase === "running" && <button className="btn" disabled><Loader2 className="h-4 w-4 animate-spin" /> Working…</button>}
            {(phase === "done" || phase === "failed") && <button className="btn" onClick={reset}>Install something else</button>}
          </div>
        </section>
      )}
    </div>
  );
}

function Banner({ tone, icon, children }: { tone: "good" | "warn" | "bad" | "info"; icon: React.ReactNode; children: React.ReactNode }) {
  const c = { good: "border-emerald-700/50 bg-emerald-950/40 text-emerald-200", warn: "border-amber-700/50 bg-amber-950/30 text-amber-200", bad: "border-rose-700/50 bg-rose-950/40 text-rose-200", info: "border-cyan-700/40 bg-cyan-950/30 text-cyan-100" }[tone];
  return <div className={`mt-3 flex items-start gap-2 rounded-xl border p-3 text-sm first:mt-0 ${c}`}><span className="mt-0.5 shrink-0">{icon}</span><div className="whitespace-pre-wrap">{children}</div></div>;
}
