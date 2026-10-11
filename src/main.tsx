import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import "./styles.css";

// Make the window behave like a desktop app, not a web page:
// no browser context menu, reload, find, print, zoom, back/forward or text selection.
const isField = (t: EventTarget | null) => {
  const el = t as HTMLElement | null;
  return !!el && (el.tagName === "INPUT" || el.tagName === "TEXTAREA" || el.isContentEditable);
};
window.addEventListener("contextmenu", (e) => { if (!isField(e.target)) e.preventDefault(); });
window.addEventListener("keydown", (e) => {
  const k = e.key.toLowerCase();
  const ctrl = e.ctrlKey || e.metaKey;
  const blocked =
    ["f3", "f5", "f7", "f12", "browserback", "browserforward", "browserrefresh", "browsersearch"].includes(k) ||
    (ctrl && ["r", "p", "u", "s", "j", "h", "g", "o", "n", "t", "w", "+", "=", "-", "0", "l", "d"].includes(k)) ||
    (ctrl && e.shiftKey && ["i", "c", "j", "r", "delete"].includes(k)) ||
    (e.altKey && (k === "arrowleft" || k === "arrowright" || k === "home"));
  if (blocked) e.preventDefault();
  if (ctrl && k === "a" && !isField(e.target)) e.preventDefault();
}, true);
window.addEventListener("wheel", (e) => { if (e.ctrlKey) e.preventDefault(); }, { passive: false });
window.addEventListener("mouseup", (e) => { if (e.button === 3 || e.button === 4) e.preventDefault(); });
window.addEventListener("auxclick", (e) => { if (e.button === 1) e.preventDefault(); });
window.addEventListener("dragstart", (e) => { if (!isField(e.target)) e.preventDefault(); });
document.addEventListener("selectstart", (e) => { if (!isField(e.target)) e.preventDefault(); });

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
