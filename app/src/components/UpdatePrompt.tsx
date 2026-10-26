import { useEffect, useState } from "react";
import { Download, RefreshCw } from "lucide-react";
import { plainNotes, type Update } from "../update";
import { Banner, Progress } from "./ui";

/** Offers a new release once at start. "Later" is remembered for that version. */
export function UpdatePrompt({ u }: { u: Update }) {
  const [closed, setClosed] = useState(false);
  const { info, offer } = u;
  const open = !!offer && !!info && !closed && offer.version !== info.skipped;
  useEffect(() => {
    if (!open || u.installing) return;
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && setClosed(true);
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [open, u.installing]);
  if (!open) return null;
  const title = `Unpack ${offer.version} is available`;
  return (
    <div className="fixed inset-x-0 bottom-0 top-9 z-40 grid place-items-center p-4">
      <div className="absolute inset-0 bg-black/50" aria-hidden />
      <div role="alertdialog" aria-modal="true" aria-label={title} className="enter card relative w-full max-w-md p-5 shadow-2xl">
        <h2 className="text-base font-semibold text-strong">{title}</h2>
        <p className="mt-2 text-dim">
          You have {info.version}. {info.self_install ? "Unpack restarts when the download finishes." : "The installer opens when the download finishes."}
        </p>
        {offer.notes && <pre className="mt-4 max-h-56 overflow-y-auto whitespace-pre-wrap rounded border border-line p-3 font-sans text-xs text-ink [overflow-wrap:anywhere]">{plainNotes(offer.notes)}</pre>}
        {u.installing && <div className="mt-4"><Progress value={u.progress} label="Downloading the update" /></div>}
        {u.error && <div className="mt-4"><Banner tone="bad" icon={<RefreshCw className="h-4 w-4" />}>{u.error}</Banner></div>}
        {u.saved && <div className="mt-4"><Banner tone="good" icon={<Download className="h-4 w-4" />}>Saved to {u.saved} and opened in your installer.</Banner></div>}
        <div className="mt-5 flex justify-end gap-2">
          <button className="btn btn-ghost" disabled={u.installing} onClick={() => { void u.skip(); setClosed(true); }}>Later</button>
          {u.saved
            ? <button className="btn btn-primary" onClick={() => setClosed(true)}>Done</button>
            : <button className="btn btn-primary" autoFocus disabled={u.installing} onClick={() => void u.install()}>
                {u.installing ? "Downloading…" : info.self_install ? "Update and restart" : "Download"}
              </button>}
        </div>
      </div>
    </div>
  );
}
