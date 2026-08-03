export function applyTheme(theme: string) {
  const next = theme === "light" ? "light" : "dark";
  document.documentElement.setAttribute("data-theme", next);
  try {
    localStorage.setItem("Streamry-theme", next);
  } catch {
    /* ignore */
  }
}

export function readCachedTheme(): "dark" | "light" {
  try {
    const t = localStorage.getItem("Streamry-theme");
    if (t === "light" || t === "dark") return t;
  } catch {
    /* ignore */
  }
  return "dark";
}
