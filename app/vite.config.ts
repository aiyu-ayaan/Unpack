import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";
import { LETTERS, LOCKUP_VIEWBOX, TRAY } from "./src/components/logoPaths";

const SQUARE = '<rect class="lg-sq" x="96" y="48" width="64" height="64" rx="10"/>'; // as in Logo.tsx

/** Fills <!--boot-logo--> in index.html with the lockup, so the splash paints before any JS runs. */
const bootLogo = {
  name: "boot-logo",
  transformIndexHtml: (html: string) =>
    html.replace("<!--boot-logo-->", `<svg viewBox="${LOCKUP_VIEWBOX}" class="lg-anim" aria-hidden><path class="lg-tray" d="${TRAY}"/>${SQUARE}${LETTERS.map((d, i) => `<path class="lg-let" d="${d}" style="--i:${i}"/>`).join("")}</svg>`),
};

export default defineConfig({
  plugins: [react(), tailwindcss(), bootLogo],
  clearScreen: false,
  server: { port: 1420, strictPort: true },
});
