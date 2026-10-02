import { test, expect } from "bun:test";
import { authorize, response, type Judgment, type Invocation } from "../../src/gate.js";

const invocation: Invocation = { command: "> notes", cwd: "/workspace", shell: "/bin/bash" };
const judged = (verdict: "allow" | "ask" | "block"): Judgment => ({ kind: verdict, reason: "guardian reason" });

// @kotowari[REQ-049, EX-082]
test("req_049_allow_preserves_the_original_permission_failure", async () => {
  const original = async () => { throw new Error("native permission denied"); };
  await expect(authorize(judged("allow"), invocation, async () => {}, original)).rejects.toThrow("native permission denied");
});

// @kotowari[REQ-049, EX-083]
test("req_049_block_never_starts_execution_even_when_approval_is_automatic", async () => {
  let started = false;
  await expect(authorize(judged("block"), invocation, async () => {}, async () => { started = true; })).rejects.toThrow("guardian reason");
  expect(started).toBe(false);
});

// @kotowari[REQ-047, REQ-050, EX-079]
test("req_050_redirect_only_waits_for_approval_before_calling_the_executor", async () => {
  let accept: () => void = () => {};
  const pending = new Promise<void>(resolve => { accept = resolve; });
  let started = false;
  const run = authorize(judged("ask"), invocation, () => pending, async () => { started = true; return "executed"; });
  await Promise.resolve();
  expect(started).toBe(false);
  accept();
  expect(await run).toBe("executed");
});

// @kotowari[REQ-048, EX-080, EX-104, EX-105]
test("req_048_confirmed_shadow_never_adds_guardian_approval_or_rejection", async () => {
  for (const reason of ["shadow", "Could not write shadow log.", "Unsupported shell"]) {
    const result = response(JSON.stringify({ status: "shadow", mode: { enforce: false }, reason }));
    expect(await authorize(result, invocation, async () => { throw new Error("must not ask"); }, async () => "native execution")).toBe("native execution");
  }
});

// @kotowari[REQ-048, REQ-051, EX-081]
test("req_051_invalid_responses_do_not_bypass_approval_as_shadow", async () => {
  for (const text of ["", "{", "null", '{"status":"shadow"}', '{"status":"judged","mode":{"enforce":false},"verdict":"allow","reason":"wrong"}']) {
    let started = false;
    await expect(authorize(response(text), invocation, async () => { throw new Error("approval rejected"); }, async () => { started = true; })).rejects.toThrow("approval rejected");
    expect(started).toBe(false);
  }
});

// @kotowari[REQ-055, EX-094, EX-095]
test("req_055_unavailable_shell_reason_requires_approval", async () => {
  const result = response(JSON.stringify({ status: "unavailable", mode: { enforce: true }, reason: "Unsupported shell: Bash only." }));
  let displayed = "";
  expect(await authorize(result, { ...invocation, shell: "/bin/zsh" }, async (_input, reason) => { displayed = reason; }, async () => "approved")).toBe("approved");
  expect(displayed).toContain("Unsupported shell");
});

// @kotowari[REQ-051]
test("req_051_unavailable_reason_is_presented_for_approval", async () => {
  let displayed = "";
  expect(await authorize({ kind: "ask", reason: "Could not establish execution input" }, invocation, async (_input, reason) => { displayed = reason; }, async () => "approved")).toBe("approved");
  expect(displayed).toContain("execution input");
});
