type Reply = { version: 1; nonce: string; kind: "budget_reply"; original_remaining_ms: number } |
  { version: 1; nonce: string; kind: "advisory_ack"; accepted: boolean };
type Exchange = { reply: Reply; deadline?: number };

function parse(text: string): Record<string, unknown> | undefined {
  try {
    const value: unknown = JSON.parse(text);
    if (typeof value !== "object" || value === null || Array.isArray(value)) return;
    const keys = new Set<string>();
    for (const match of text.matchAll(/"((?:[^"\\]|\\.)*)"\s*:/g)) {
      const key: string = JSON.parse(`"${match[1]}"`);
      if (keys.has(key)) return;
      keys.add(key);
    }
    return value as Record<string, unknown>;
  } catch { return; }
}

export class BudgetControl {
  private phase: "new" | "probed" | "finished" = "new";
  private nonce?: string;
  constructor(private readonly originalDeadline: number) {}

  cancel(): void { this.phase = "finished"; }

  receive(text: string, now: number): Exchange | undefined {
    if (this.phase === "finished") return;
    const value = parse(text);
    const phase = this.phase;
    this.phase = "finished";
    if (!value || value.version !== 1 || typeof value.nonce !== "string" || !/^[0-9a-f]{32}$/.test(value.nonce) || !Number.isFinite(now)) return;
    const nonce = value.nonce;
    if (phase === "new" && value.kind === "budget_probe" && Object.keys(value).length === 3) {
      const remaining = Math.floor(this.originalDeadline-now);
      if (!Number.isSafeInteger(remaining) || remaining <= 0) return;
      this.nonce = nonce;
      this.phase = "probed";
      return {reply:{version:1,nonce,kind:"budget_reply",original_remaining_ms:remaining}};
    }
    if (phase === "probed" && value.kind === "advisory_start" && nonce === this.nonce && Object.keys(value).length === 4) {
      const timeout = value.timeout_ms;
      if (typeof timeout !== "number" || !Number.isSafeInteger(timeout) || timeout <= 0) return;
      const deadline = now+timeout+1000;
      if (!Number.isFinite(deadline) || deadline > Number.MAX_SAFE_INTEGER) return;
      const accepted = this.originalDeadline-now > 500;
      return {reply:{version:1,nonce,kind:"advisory_ack",accepted},...(accepted ? {deadline} : {})};
    }
  }
}
