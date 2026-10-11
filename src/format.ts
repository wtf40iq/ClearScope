const UNITS = ["Б", "КБ", "МБ", "ГБ", "ТБ", "ПБ"];

export function fmtSize(bytes: number, digits = 1): string {
  if (!isFinite(bytes)) return "—";
  const neg = bytes < 0;
  let b = Math.abs(bytes);
  let i = 0;
  while (b >= 1024 && i < UNITS.length - 1) {
    b /= 1024;
    i++;
  }
  const s = i === 0 ? String(Math.round(b)) : b.toFixed(b >= 100 ? 0 : digits);
  return `${neg ? "−" : ""}${s.replace(".", ",")} ${UNITS[i]}`;
}

export function fmtDelta(bytes: number): string {
  if (bytes === 0) return "0";
  return (bytes > 0 ? "+" : "") + fmtSize(bytes);
}

export function fmtNum(n: number): string {
  return n.toLocaleString("ru-RU");
}

export function fmtDate(unix: number): string {
  if (!unix) return "—";
  const d = new Date(unix * 1000);
  return d.toLocaleDateString("ru-RU", { day: "2-digit", month: "2-digit", year: "numeric" });
}

export function fmtDateTime(unix: number): string {
  if (!unix) return "—";
  return new Date(unix * 1000).toLocaleString("ru-RU", { day: "2-digit", month: "short", hour: "2-digit", minute: "2-digit" });
}

export function fmtMs(ms: number): string {
  if (ms < 1000) return `${ms} мс`;
  if (ms < 60000) return `${(ms / 1000).toFixed(1).replace(".", ",")} с`;
  const m = Math.floor(ms / 60000);
  return `${m} мин ${Math.round((ms % 60000) / 1000)} с`;
}

export function pct(part: number, whole: number): number {
  return whole > 0 ? (part / whole) * 100 : 0;
}

export function fmtPct(p: number): string {
  if (p >= 10) return `${p.toFixed(0)}%`;
  if (p >= 0.1) return `${p.toFixed(1).replace(".", ",")}%`;
  return p > 0 ? "<0,1%" : "0%";
}

export function fmtAgo(unix: number): string {
  if (!unix) return "—";
  const s = Math.max(0, Math.floor(Date.now() / 1000) - unix);
  if (s < 60) return "только что";
  if (s < 3600) return `${Math.floor(s / 60)} мин назад`;
  if (s < 86400) return `${Math.floor(s / 3600)} ч назад`;
  if (s < 86400 * 7) return `${Math.floor(s / 86400)} дн назад`;
  return fmtDate(unix);
}
