import { useEffect } from "react";

/** Start-up screen. The markup lives in index.html so it paints before React loads; this only closes it. Click to skip. */
export function Splash() {
  useEffect(() => {
    const boot = document.getElementById("boot");
    if (!boot) return;
    const reduced = matchMedia("(prefers-reduced-motion: reduce)").matches;
    const out = () => boot.classList.add("out");
    const t1 = setTimeout(out, reduced ? 400 : 2800);
    const t2 = setTimeout(() => boot.remove(), reduced ? 600 : 3200);
    boot.addEventListener("click", out);
    return () => { clearTimeout(t1); clearTimeout(t2); boot.removeEventListener("click", out); };
  }, []);
  return null;
}
