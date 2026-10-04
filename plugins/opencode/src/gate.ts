export interface Invocation {
  readonly command: string;
  readonly cwd: string;
  readonly shell: string;
}

export type Judgment =
  | { readonly kind: "allow" | "ask" | "block" | "shadow"; readonly reason: string }
  | { readonly kind: "deferred"; readonly reason: string; readonly warning?: string };

export function response(text: string): Judgment {
  const invalid: Judgment = { kind: "ask", reason: "Invalid guardian response." };
  let value: unknown;
  try { value = JSON.parse(text); } catch { return invalid; }
  if (typeof value !== "object" || value === null || !("status" in value) || !("reason" in value) || typeof value.reason !== "string") return invalid;
  const enforce = "mode" in value && typeof value.mode === "object" && value.mode !== null && "enforce" in value.mode ? value.mode.enforce : undefined;
  if (value.status === "shadow" && enforce === false && !("verdict" in value)) return { kind: "shadow", reason: value.reason };
  if (value.status === "judged" && enforce === true && "verdict" in value && (value.verdict === "allow" || value.verdict === "ask" || value.verdict === "block")) return { kind: value.verdict, reason: value.reason };
  if (value.status === "deferred" && enforce === true && !("verdict" in value)) {
    if (!("warning" in value)) return { kind: "deferred", reason: value.reason };
    if (typeof value.warning === "string") return { kind: "deferred", reason: value.reason, warning: value.warning };
  }
  if (value.status === "unavailable" && enforce !== false && !("verdict" in value)) return { kind: "ask", reason: value.reason };
  return invalid;
}

export async function authorize<T>(
  judgment: Judgment,
  input: Invocation,
  approve: (input: Invocation, reason: string) => Promise<void>,
  execute: () => Promise<T>,
): Promise<T> {
  switch (judgment.kind) {
    case "block": throw new Error(judgment.reason);
    case "ask": await approve(input, judgment.reason); break;
    case "deferred": if (judgment.warning !== undefined) console.warn(judgment.warning); break;
    case "shadow":
    case "allow": break;
  }
  return execute();
}
