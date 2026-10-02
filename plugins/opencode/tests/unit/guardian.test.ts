import { test, expect } from "bun:test";
import { mkdtemp, writeFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { judge, executionInput } from "../../src/guardian.js";

const input = { command: "echo '$HOME'; > notes", cwd: "/workspace", shell: "/bin/bash" };

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
