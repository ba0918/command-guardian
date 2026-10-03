import type { Invocation, Judgment } from "./gate.js";
import { response } from "./gate.js";
import { spawn } from "node:child_process";
import { resolve, isAbsolute } from "node:path";
import { homedir } from "node:os";
import { Duplex } from "node:stream";
import { BudgetControl } from "./advisor-control.js";

export async function judge(binary:string,input:Invocation,signal:AbortSignal):Promise<Judgment> {
  if (signal.aborted) throw new Error("Guardian execution cancelled.");
  return new Promise((finish,reject) => {
    const originalDeadline = performance.now()+6000;
    const budget = new BudgetControl(originalDeadline);
    const child = spawn(binary,["hook","--agent","opencode"],{stdio:["pipe","pipe","pipe","pipe"],detached:true,
      env:{...process.env,COMMAND_GUARDIAN_ADVISOR_CONTROL:"1"}});
    let output = "", warnings = "", settled = false;
    const stop = () => { if (child.pid) { try { process.kill(-child.pid,"SIGKILL"); } catch {} } };
    const control = child.stdio[3];
    let timer: ReturnType<typeof setTimeout>;
    const cleanup = () => { clearTimeout(timer); budget.cancel(); if (control instanceof Duplex) control.destroy(); signal.removeEventListener("abort",abort); };
    const abort = () => { if (settled) return; settled=true; cleanup(); stop(); reject(new Error("Guardian execution cancelled.")); };
    const arm = (deadline:number) => {
      clearTimeout(timer);
      const remaining = deadline-performance.now();
      timer=setTimeout(() => {
        if (settled) return;
        if (performance.now() < deadline) { arm(deadline); return; }
        stop(); complete({kind:"ask",reason:"Guardian process timed out."});
      },Math.min(2147483647,Math.max(0,remaining)));
    };
    const complete = (result:Judgment) => {
      if (settled) return;
      settled = true; cleanup(); finish(result);
    };
    arm(originalDeadline);
    if (control instanceof Duplex) {
      let buffer="";
      control.setEncoding("utf8");
      control.on("error",()=>{budget.cancel();});
      control.on("data",(data:string)=>{
        if (settled || signal.aborted) return;
        buffer+=data;
        if (buffer.length > 4096) { budget.cancel(); control.destroy(); return; }
        let end:number;
        while ((end=buffer.indexOf("\n"))>=0) {
          const exchange=budget.receive(buffer.slice(0,end),performance.now());
          buffer=buffer.slice(end+1);
          if (!exchange) continue;
          if (exchange.deadline !== undefined) arm(exchange.deadline);
          control.write(JSON.stringify(exchange.reply)+"\n");
        }
      });
    }
    signal.addEventListener("abort",abort,{once:true});
    if (signal.aborted) abort();
    child.on("error",error => complete({kind:"ask",reason: "code" in error && error.code === "ENOENT" ? "Guardian binary not found." : "Could not start guardian process."}));
    child.stdin?.on("error",() => {});
    child.stdout?.setEncoding("utf8").on("data",(data:string) => {
      output += data;
      if (output.length > 1024*1024) { stop(); complete({kind:"ask",reason:"Guardian response exceeded output limit."}); }
    });
    child.stderr?.setEncoding("utf8").on("data",(data:string) => { warnings = (warnings+data).slice(-65536); });
    child.on("close",code => {
      if (warnings) console.warn(warnings.trim());
      complete(code === 0 ? response(output) : {kind:"ask",reason:"Guardian process failed."});
    });
    child.stdin?.end(JSON.stringify(input));
  });
}
export function executionInput(input:unknown,directory:string,shell:string):Invocation|undefined {
  // Do not narrow the host's string schema: an empty command can require approval.
  if (
    typeof input !== "object" || input === null || !("command" in input) ||
    typeof input.command !== "string" || !isAbsolute(directory) || !isAbsolute(shell)
  ) return undefined;
  const workdir = "workdir" in input ? input.workdir : directory;
  if (workdir !== undefined && typeof workdir !== "string") return undefined;
  const cwd = workdir ?? directory;
  // Match the host's Linux lexical resolution; do not dereference symlinks.
  const expanded = cwd === "~" ? homedir() : cwd.startsWith("~/") ? resolve(homedir(),cwd.slice(2)) : cwd;
  return {command:input.command,cwd:resolve(directory,expanded),shell};
}
