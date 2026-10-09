import { formatCount } from "../../catalog/catalogMessages";

export function yesNo(value: boolean): string {
  return value ? "Yes" : "No";
}

/** Formats a size in bytes, keeping the exact value. */
export function formatSize(bytes: number | null): string | null {
  if (bytes === null) return null;
  if (bytes < 1024) return `${formatCount(bytes)} B`;
  const units = ["KiB", "MiB", "GiB"];
  let value = bytes / 1024;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit++;
  }
  const rounded = Number.isInteger(value) ? String(value) : value.toFixed(1);
  return `${rounded} ${units[unit] ?? ""} (${formatCount(bytes)} B)`;
}

export function formatClock(hertz: number | null): string | null {
  if (hertz === null) return null;
  const scaled = (value: number, unit: string) =>
    `${value.toFixed(6).replace(/\.?0+$/, "")} ${unit}`;
  if (hertz >= 1_000_000) return scaled(hertz / 1_000_000, "MHz");
  if (hertz >= 1_000) return scaled(hertz / 1_000, "kHz");
  return `${String(hertz)} Hz`;
}

export function joinPresent(values: (string | null | undefined)[], separator = " · "): string {
  return values.filter((value): value is string => Boolean(value)).join(separator);
}
