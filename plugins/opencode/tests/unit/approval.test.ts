import { test, expect } from "bun:test";
import { Approval, type ApprovalEvent, type ApprovalRequest } from "../../src/approval.js";

class Events {
  values: ApprovalEvent[] = [{ type: "connected" }];
  wake: (() => void) | undefined;
  closed = false;
  emit(event: ApprovalEvent) { this.values.push(event); this.wake?.(); }
  end() { this.closed = true; this.wake?.(); }
  async *stream() {
    while (!this.closed) {
      const value = this.values.shift();
      if (value) yield value;
      else await new Promise<void>(resolve => { this.wake = resolve; });
    }
  }
}

const input = { command: "> notes", cwd: "/workspace", shell: "/bin/bash" };

// @kotowari[REQ-053]
test("req_053_cancellation_before_event_connection_settles_without_a_deadline",async()=>{
  const approval=new Approval({
    newID:()=>"unused",
    events:async function*(signal){
      await new Promise<void>(resolve=>signal.addEventListener("abort",()=>resolve(),{once:true}));
    },
    create:async()=>{throw new Error("Disconnected stream cannot create approval");},
    reject:async()=>{},
  });
  const controller=new AbortController();
  const result=approval.request("session",input,"reason",controller.signal).then(()=>"approved",error=>error);
  controller.abort();
  try{
    const settled=await Promise.race([result,new Promise(resolve=>setTimeout(()=>resolve("not cancelled"),100))]);
    expect(settled).toBeInstanceOf(Error);
  }finally{approval.close();}
});
function fixture(create?: (request: ApprovalRequest) => Promise<"ask" | "allow" | "deny">, reconnect = true) {
  const streams: Events[] = [];
  const requests: ApprovalRequest[] = [];
  const removed: string[] = [];
  let next=0, connections=0;
  const approval = new Approval({
    newID:()=>String(++next),
    events: () => {
      connections++;
      if (streams.length > 0 && !reconnect) throw new TypeError("fetch failed");
      const events = new Events(); streams.push(events); return events.stream();
    },
    create: async request => { requests.push(request); return create ? create(request) : "ask"; },
    reject: async id => { removed.push(id); },
  });
  return { get events() { const latest = streams.at(-1); if (!latest) throw new Error("no event stream"); return latest; }, get connections() { return connections; }, streams, requests, removed, approval };
}
async function waitFor(check: () => boolean) {
  for (let i = 0; i < 100; i++) { if (check()) return; await Promise.resolve(); }
  throw new Error("expected request did not arrive");
}

// @kotowari[REQ-050, EX-084]
test("req_050_early_once_reply_before_create_returns_is_not_lost", async () => {
  const f = fixture(async request => { f.events.emit({ type: "reply", id: request.id, session: request.session, reply: "once" }); return "ask"; });
  await f.approval.request("session", input, "reason", new AbortController().signal);
  expect(f.requests).toHaveLength(1);
  f.approval.close(); f.events.end();
});

// @kotowari[REQ-050, EX-085]
test("req_050_each_execution_creates_a_fresh_request_after_always", async () => {
  const f = fixture(async request => { f.events.emit({ type: "reply", id: request.id, session: request.session, reply: "always" }); return "ask"; });
  for (let i = 0; i < 2; i++) await f.approval.request("session", input, "reason", new AbortController().signal);
  expect(f.requests).toHaveLength(2);
  expect(f.requests[0]?.id).not.toBe(f.requests[1]?.id);
  f.approval.close(); f.events.end();
});

// @kotowari[REQ-067, EX-147]
test("req_067_creation_failure_is_left_to_opencode_not_approved_or_stopped", async () => {
  const f = fixture(async () => { throw new Error("authentication failed"); });
  expect(await f.approval.request("session", input, "reason", new AbortController().signal)).toBe("unavailable");
  f.approval.close(); f.events.end();
});

// @kotowari[REQ-067, EX-149]
test("ex_149_connection_lost_before_the_creation_response_is_left_to_opencode", async () => {
  let fail: (error: Error) => void = () => {};
  const f = fixture(() => new Promise((_resolve, reject) => { fail = reject; }));
  const run = f.approval.request("session", input, "reason", new AbortController().signal);
  await waitFor(() => f.requests.length === 1);
  f.events.end();
  fail(new TypeError("fetch failed"));
  expect(await run).toBe("unavailable");
  f.approval.close();
});

// @kotowari[REQ-067, EX-151]
test("ex_151_reject_reply_before_a_failed_creation_is_kept", async () => {
  let fail: (error: Error) => void = () => {};
  const f = fixture(() => new Promise((_resolve, reject) => { fail = reject; }));
  const run = f.approval.request("session", input, "reason", new AbortController().signal).then(() => null, error => error);
  await waitFor(() => f.requests.length === 1);
  const request = f.requests[0]; if (!request) throw new Error("missing request");
  f.events.emit({ type: "reply", id: request.id, session: "session", reply: "reject" });
  await new Promise(resolve => setTimeout(resolve, 0));
  fail(new Error("authentication failed"));
  expect(await run).toBeInstanceOf(Error);
  f.approval.close(); f.events.end();
});

// @kotowari[REQ-067, EX-152]
test("ex_152_approval_after_the_event_connection_ended_reconnects_and_waits", async () => {
  const f = fixture();
  f.events.end();
  await new Promise(resolve => setTimeout(resolve, 0));
  let done = false;
  const run = f.approval.request("session", input, "reason", new AbortController().signal).then(result => { done = true; return result; });
  await waitFor(() => f.requests.length === 1);
  expect(f.streams).toHaveLength(2);
  await new Promise(resolve => setTimeout(resolve, 0));
  expect(done).toBe(false);
  const request = f.requests[0]; if (!request) throw new Error("missing request");
  f.events.emit({ type: "reply", id: request.id, session: "session", reply: "once" });
  expect(await run).toBe("approved");
  f.approval.close(); f.events.end();
});

// @kotowari[REQ-067, EX-153]
test("ex_153_approval_after_the_event_connection_ended_without_reconnection_is_left_to_opencode", async () => {
  const f = fixture(undefined, false);
  const events = f.events;
  f.events.end();
  await new Promise(resolve => setTimeout(resolve, 0));
  expect(await f.approval.request("session", input, "reason", new AbortController().signal)).toBe("unavailable");
  expect(f.connections).toBe(2);
  expect(f.requests).toHaveLength(0);
  f.approval.close(); events.end();
});

// @kotowari[REQ-067, EX-150]
test("ex_150_creation_answered_after_its_connection_ended_is_cancelled_even_after_reconnection", async () => {
  let answer: (value: "ask") => void = () => {};
  let first = true;
  const f = fixture(() => first ? (first = false, new Promise(resolve => { answer = resolve; })) : Promise.resolve("ask"));
  const run = f.approval.request("session", input, "reason", new AbortController().signal).then(() => null, error => error);
  await waitFor(() => f.requests.length === 1);
  f.events.end();
  await new Promise(resolve => setTimeout(resolve, 0));
  const second = f.approval.request("session", input, "reason", new AbortController().signal);
  await waitFor(() => f.requests.length === 2);
  answer("ask");
  expect(await run).toBeInstanceOf(Error);
  const later = f.requests[1]; if (!later) throw new Error("missing request");
  f.events.emit({ type: "reply", id: later.id, session: "session", reply: "once" });
  expect(await second).toBe("approved");
  f.approval.close(); f.events.end();
});

// @kotowari[REQ-067, REQ-053, EX-150]
test("req_067_creation_answered_after_the_event_connection_ended_is_cancelled", async () => {
  let answer: (value: "ask") => void = () => {};
  const f = fixture(() => new Promise(resolve => { answer = resolve; }));
  const run = f.approval.request("session", input, "reason", new AbortController().signal).then(() => null, error => error);
  await waitFor(() => f.requests.length === 1);
  f.events.end();
  answer("ask");
  expect(await run).toBeInstanceOf(Error);
  const request = f.requests[0]; if (!request) throw new Error("missing request");
  expect(f.removed).toContain(request.id);
  f.approval.close();
});

// @kotowari[REQ-050]
test("req_050_native_rejection_does_not_become_approval", async () => {
  const f = fixture(async request => { f.events.emit({type:"reply",id:request.id,session:request.session,reply:"reject"}); return "ask"; });
  await expect(f.approval.request("session",input,"reason",new AbortController().signal)).rejects.toThrow("rejected");
  f.approval.close(); f.events.end();
});

// @kotowari[REQ-053, EX-090]
test("req_053_wait_has_no_independent_approval_deadline", async () => {
  const f = fixture();
  let done = false;
  const run = f.approval.request("session", input, "reason", new AbortController().signal).then(() => { done = true; });
  await waitFor(() => f.requests.length === 1);
  await new Promise(resolve => setTimeout(resolve, 20));
  expect(done).toBe(false);
  const request = f.requests[0]; if (!request) throw new Error("missing request");
  f.events.emit({type:"reply",id:request.id,session:"session",reply:"once"});
  await run;
  f.approval.close(); f.events.end();
});

// @kotowari[REQ-053, REQ-054, EX-091, EX-093, EX-148]
test("req_053_abort_disconnect_and_unload_never_replay_late_approvals", async () => {
  for (const cancellation of ["abort", "disconnect", "unload"]) {
    const f = fixture(); const controller = new AbortController();
    const run = f.approval.request("session",input,"reason",controller.signal);
    const rejected = run.then(() => null, error => error);
    await waitFor(() => f.requests.length === 1);
    // Waiting starts once the creation response has been received.
    await new Promise(resolve => setTimeout(resolve, 0));
    if (cancellation === "abort") controller.abort();
    else if (cancellation === "disconnect") f.events.end();
    else f.approval.close();
    expect(await rejected).toBeInstanceOf(Error);
    const request = f.requests[0]; if (!request) throw new Error("missing request");
    f.events.emit({type:"reply",id:request.id,session:"session",reply:"once"});
    expect(f.removed).toContain(request.id);
    f.approval.close(); f.events.end();
  }
});

// @kotowari[REQ-053]
test("req_053_abort_during_creation_is_final_even_if_creation_later_succeeds", async () => {
  let finish: (value:"ask") => void = () => {};
  const pending = new Promise<"ask">(resolve => { finish = resolve; });
  const f = fixture(() => pending); const controller = new AbortController();
  const run = f.approval.request("session",input,"reason",controller.signal);
  const rejected = run.then(() => null, error => error);
  await waitFor(() => f.requests.length === 1); controller.abort();
  finish("ask"); expect(await rejected).toBeInstanceOf(Error);
  expect(f.removed.length).toBeGreaterThan(0);
  f.approval.close(); f.events.end();
});

// @kotowari[REQ-054, REQ-047, EX-092, EX-102]
test("req_054_parallel_requests_keep_their_own_cwd_and_approval", async () => {
  const f = fixture(); const controller = new AbortController(); let secondDone = false;
  const first = f.approval.request("session",input,"reason",controller.signal);
  const second = f.approval.request("session",{...input,cwd:"/other"},"reason",controller.signal).then(() => {secondDone=true;});
  await waitFor(() => f.requests.length === 2);
  const a=f.requests[0], b=f.requests[1]; if (!a||!b) throw new Error("missing requests");
  expect(a.input.cwd).toBe("/workspace"); expect(b.input.cwd).toBe("/other");
  f.events.emit({type:"reply",id:a.id,session:"wrong-session",reply:"once"});
  f.events.emit({type:"reply",id:a.id,session:"session",reply:"once"}); await first;
  expect(secondDone).toBe(false);
  f.events.emit({type:"reply",id:b.id,session:"session",reply:"once"}); await second;
  f.approval.close(); f.events.end();
});
