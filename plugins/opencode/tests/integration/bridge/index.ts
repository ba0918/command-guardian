import { Plugin, Rpc, Agent } from "@opencode/plugin";
import { Session } from "@opencode/schema/session";
import { SessionMessage } from "@opencode/schema/session-message";
import { Tool } from "@opencode/schema/tool";
import { CodeMode, Tool as CodeModeTool } from "@opencode/codemode";
import { Effect } from "effect";

export default Plugin.define({
  id: "guardian.test.bridge",
  async setup(ctx) {
    await ctx.rpc.register(Rpc.define({id:"guardian-test",events:{},methods:{execute:{
      input:{type:"object",properties:{session:{type:"string"},command:{type:"string"},workdir:{type:"string"},background:{type:"boolean"},codeMode:{type:"boolean"}},required:["session","command"]},
      output:{type:"object",properties:{error:{type:"string"},content:{type:"string"}}},
    }}}), {execute: async (input,rpc) => {
      if (typeof input!=="object" || input===null || !("session" in input) || typeof input.session!=="string" || !("command" in input) || typeof input.command!=="string") throw new Error("Invalid execution input");
      const shell=(await ctx.tool.list()).find(tool=>tool.id==="shell");
      if (!shell) throw new Error("Host shell unavailable");
      const value={command:input.command,...("workdir" in input && typeof input.workdir==="string"?{workdir:input.workdir}:{}),...("background" in input && typeof input.background==="boolean"?{background:input.background}:{})};
      try {
        const context={sessionID:Session.ID.make(input.session),agent:Agent.ID.make("build"),messageID:SessionMessage.ID.create(),id:Tool.CallID.make(crypto.randomUUID()),signal:rpc.signal,progress:async()=>{}};
        if("codeMode" in input && input.codeMode===true){
          const result=await Effect.runPromise(CodeMode.execute({
            code:`return await tools.shell(${JSON.stringify(value)});`,
            tools:{shell:CodeModeTool.make({description:shell.description,input:{type:"object",properties:{command:{type:"string"},workdir:{type:"string"}}},output:{type:"string"},execute:input=>Effect.tryPromise(()=>shell.execute(input,context).then(result=>JSON.stringify(result.content)))})},
          }),{signal:rpc.signal});
          return {content:JSON.stringify(result)};
        }
        const result=await shell.execute(value,context);
        return {content:typeof result.content==="string"?result.content:JSON.stringify(result.content)};
      } catch(error) {return {error:error instanceof Error?error.message:String(error)};}
    }});
  },
});
