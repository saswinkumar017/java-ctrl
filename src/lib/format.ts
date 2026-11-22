const BYTE_UNITS = ["B", "KiB", "MiB", "GiB", "TiB", "PiB"];

const numberFormatters = new Map<number, Intl.NumberFormat>();
const compactFormatter = new Intl.NumberFormat("en", { notation: "compact", maximumFractionDigits: 1 });

function formatter(digits: number): Intl.NumberFormat {
  let cached = numberFormatters.get(digits);
  if (!cached) {
    cached = new Intl.NumberFormat("en", { minimumFractionDigits: digits, maximumFractionDigits: digits });
    numberFormatters.set(digits, cached);
  }
  return cached;
}

export function formatBytes(bytes: number | null | undefined, digits = 1): string {
  if (bytes == null || !Number.isFinite(bytes)) return "—";
  if (bytes < 1024) return `${Math.round(bytes)} B`;
  let value = bytes;
  let unit = 0;
  while (value >= 1024 && unit < BYTE_UNITS.length - 1) {
    value /= 1024;
    unit += 1;
  }
  return `${formatter(value >= 100 ? 0 : digits).format(value)} ${BYTE_UNITS[unit] ?? "B"}`;
}

export function formatNumber(value: number | null | undefined, digits = 0): string {
  if (value == null || !Number.isFinite(value)) return "—";
  return formatter(digits).format(value);
}

export function formatCompact(value: number | null | undefined): string {
  if (value == null || !Number.isFinite(value)) return "—";
  if (Math.abs(value) < 1000) return formatNumber(value);
  return compactFormatter.format(value);
}

export function formatPercent(value: number | null | undefined, digits = 1): string {
  if (value == null || !Number.isFinite(value)) return "—";
  return `${formatter(digits).format(value)}%`;
}

export function percentOf(used: number, max: number): number {
  if (!max || !Number.isFinite(max) || max <= 0) return 0;
  return Math.min(100, Math.max(0, (used / max) * 100));
}

export function formatDuration(secs: number | null | undefined): string {
  if (secs == null || !Number.isFinite(secs) || secs < 0) return "—";
  if (secs < 60) return `${Math.round(secs)}s`;
  const minutes = Math.floor(secs / 60);
  if (minutes < 60) return `${minutes}m ${Math.round(secs % 60)}s`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `${hours}h ${minutes % 60}m`;
  const days = Math.floor(hours / 24);
  return `${days}d ${hours % 24}h`;
}

export function formatMs(ms: number | null | undefined): string {
  if (ms == null || !Number.isFinite(ms) || ms < 0) return "—";
  if (ms < 1000) return `${Math.round(ms)} ms`;
  if (ms < 60_000) return `${formatter(ms < 10_000 ? 2 : 1).format(ms / 1000)} s`;
  return formatDuration(ms / 1000);
}

export function formatTime(timestamp: number | null | undefined): string {
  if (timestamp == null) return "—";
  return new Intl.DateTimeFormat("en", {
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
    hour12: false,
  }).format(new Date(timestamp));
}

export function formatDateTime(timestamp: number | null | undefined): string {
  if (timestamp == null) return "—";
  return new Intl.DateTimeFormat("en", {
    month: "short",
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
    hour12: false,
  }).format(new Date(timestamp));
}

export function formatDate(timestamp: number | null | undefined): string {
  if (timestamp == null) return "—";
  return new Intl.DateTimeFormat("en", {
    year: "numeric",
    month: "short",
    day: "numeric",
  }).format(new Date(timestamp));
}

export function formatRelative(timestamp: number | null | undefined, now = Date.now()): string {
  if (timestamp == null) return "—";
  const seconds = Math.max(0, Math.round((now - timestamp) / 1000));
  if (seconds < 5) return "just now";
  if (seconds < 60) return `${seconds}s ago`;
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) return `${minutes}m ago`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `${hours}h ago`;
  const days = Math.floor(hours / 24);
  return `${days}d ago`;
}

export function clamp(value: number, min: number, max: number): number {
  return Math.min(max, Math.max(min, value));
}

export function shorten(text: string, max = 70): string {
  if (text.length <= max) return text;
  const head = Math.ceil((max - 1) / 2);
  const tail = Math.floor((max - 1) / 2);
  return `${text.slice(0, head)}…${text.slice(text.length - tail)}`;
}

export function metricValue(value: number, baseUnit: string | null | undefined): string {
  if (!Number.isFinite(value)) return "—";
  switch (baseUnit) {
    case "bytes":
      return formatBytes(value);
    case "seconds":
      return formatMs(value * 1000);
    case "milliseconds":
      return formatMs(value);
    default:
      return formatNumber(value, Number.isInteger(value) ? 0 : 2);
  }
}

export function plural(count: number, singular: string, pluralForm = `${singular}s`): string {
  return `${formatNumber(count)} ${count === 1 ? singular : pluralForm}`;
}
