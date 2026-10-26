// Runs before first paint so the window never flashes white. Mirrors useTheme() in src/App.tsx.
(function () {
  var t;
  try { t = localStorage.getItem("theme"); } catch (e) { /* storage blocked */ }
  if (t !== "dark" && t !== "light") t = matchMedia("(prefers-color-scheme: light)").matches ? "light" : "dark";
  var d = document.documentElement;
  d.dataset.theme = t;
  d.style.background = t === "light" ? "#ffffff" : "#1f1f1f";
  d.style.colorScheme = t;
})();
