import { test, expect } from "bun:test";
import { mkdtemp, writeFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { judge, executionInput } from "../../src/guardian.js";
import { authorize } from "../../src/gate.js";

const input = { command: "echo '$HOME'; > notes", cwd: "/workspace", shell: "/bin/bash" };

const controlChild = `#!/usr/bin/python3
import json,socket,time,sys
s=socket.socket(fileno=3)
nonce='07070707070707070707070707070707'
def send(value):s.sendall((json.dumps(value)+'\\n').encode())
def receive():
 data=b''
 while not data.endswith(b'\\n'):data+=s.recv(1)
 return json.loads(data)
send({'version':1,'nonce':nonce,'kind':'budget_probe'})
probe=receive()
assert probe['kind']=='budget_reply' and probe['original_remaining_ms']>0
send({'version':1,'nonce':nonce,'kind':'advisory_start','timeout_ms':10000})
ack=receive()
assert ack['kind']=='advisory_ack' and ack['accepted']
time.sleep(6.1)
print(json.dumps({'status':'judged','mode':{'enforce':True},'verdict':'allow','reason':'fixture result'}))
`;

// @kotowari[REQ-advisor-020, EX-advisor-039]
test("actual_inherited_control_fd_extends_timer_without_putting_control_on_stdout", async () => {
  const dir = await mkdtemp(join(tmpdir(), "guardian-control-"));
  const file = join(dir,"peer");
  await writeFile(file,controlChild,{mode:0o700});
  try {
    const result = await judge(file,input,new AbortController().signal);
    expect(result.kind).toBe("allow");
    expect(result.reason).toBe("fixture result");
  } finally { await rm(dir,{recursive:true,force:true}); }
},15000);

// @kotowari[REQ-advisor-016, REQ-advisor-020, EX-advisor-039]
test("accepted_timeouts_above_the_platform_timer_range_preserve_the_judged_result", async () => {
  const dir = await mkdtemp(join(tmpdir(), "guardian-control-long-"));
  const file = join(dir,"peer");
  try {
    for (const timeout of [2147483648,18446744073709]) {
      await writeFile(file,controlChild.replace("'timeout_ms':10000",`'timeout_ms':${timeout}`).replace("time.sleep(6.1)","time.sleep(0.2)"),{mode:0o700});
      const result = await judge(file,input,new AbortController().signal);
      expect(result.kind).toBe("allow");
      expect(result.reason).toBe("fixture result");
    }
  } finally { await rm(dir,{recursive:true,force:true}); }
});

// @kotowari[REQ-advisor-020, REQ-053, EX-advisor-047]
test("cancelled_execution_cannot_resume_after_a_control_ack_or_late_result", async () => {
  const dir = await mkdtemp(join(tmpdir(), "guardian-control-cancel-"));
  const file = join(dir,"peer");
  try {
    for (const timeout of [10000,2147483648,18446744073709]) {
      await writeFile(file,controlChild.replace("'timeout_ms':10000",`'timeout_ms':${timeout}`),{mode:0o700});
      const controller = new AbortController();
      const result = judge(file,input,controller.signal);
      const timer = setTimeout(()=>controller.abort(),100);
      try { await expect(result).rejects.toThrow("cancelled"); } finally { clearTimeout(timer); }
    }
  } finally { await rm(dir,{recursive:true,force:true}); }
});

// @kotowari[REQ-051, EX-086]
test("req_051_missing_binary_gives_an_approval_reason", async () => {
  const result = await judge("/nonexistent/guardian-fixture", input, new AbortController().signal);
  expect(result.kind).toBe("ask"); expect(result.reason).toContain("not found");
});

// @kotowari[REQ-051, EX-087]
test("req_051_hung_binary_times_out_without_a_safe_verdict", async () => {
  const dir = await mkdtemp(join(tmpdir(), "guardian-timeout-"));
  const file = join(dir, "hung");
  await writeFile(file, "#!/bin/bash\nexec sleep 60\n", { mode: 0o700 });
  try {
    const result = await judge(file,input,new AbortController().signal);
    expect(result.kind).toBe("ask"); expect(result.reason).toContain("timed out");
  } finally { await rm(dir,{recursive:true,force:true}); }
},10000);

// @kotowari[REQ-047, EX-078, EX-102]
test("req_047_normal_and_background_inputs_keep_lexical_cwd_and_exact_command", () => {
  for (const background of [false,true]) {
    const raw = { command: input.command, workdir: "sub/../other", background };
    expect(executionInput(raw,"/workspace","/bin/bash")).toEqual({...input,cwd:"/workspace/other"});
    expect(executionInput({command:input.command,background},"/workspace","/bin/bash")).toEqual(input);
    expect(raw.workdir).toBe("sub/../other");
  }
});

// @kotowari[REQ-061, REQ-051, EX-125]
test("req_061_missing_binary_is_not_deferred_and_asks_with_the_reason", async () => {
  const result = await judge("/nonexistent/guardian-fixture", input, new AbortController().signal);
  let displayed = "";
  let started = false;
  await authorize(result, input, async (_input, reason) => { displayed = reason; }, async () => { started = true; });
  expect(displayed).toContain("not found");
  expect(started).toBe(true);
});
