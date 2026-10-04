import { Plugin } from "@opencode/plugin";
import type { OpenCodeClient } from "@opencode/client";
import { Error as ToolError } from "@opencode/plugin/promise/tool";
import type { ToolContext } from "@opencode/plugin/promise/tool";
import { Permission } from "@opencode/schema/permission";
import { AsyncLocalStorage } from "node:async_hooks";
import { stat } from "node:fs/promises";
import { isAbsolute } from "node:path";
import { Approval, type ApprovalEvent } from "./approval.js";
import { authorize, type Invocation } from "./gate.js";
import { executionInput, judge } from "./guardian.js";
import { connect } from "./connection.js";

// The plugin API the plugin calls; its presence, not the OpenCode version, decides support.
const requiredApis:readonly (readonly [string,(ctx:Plugin.Context)=>unknown,"function"|"string"])[]=[
  ["shell.hook",ctx=>ctx.shell?.hook,"function"],
  ["permission.hook",ctx=>ctx.permission?.hook,"function"],
  ["permission.reply",ctx=>ctx.permission?.reply,"function"],
  ["tool.transform",ctx=>ctx.tool?.transform,"function"],
  ["location.directory",ctx=>ctx.location?.directory,"string"],
];

function missingApis(ctx:Plugin.Context):string[] {
  return requiredApis.filter(([,read,type])=>typeof read(ctx)!==type).map(([name])=>name);
}

async function configuredShell(client:OpenCodeClient,directory:string,signal:AbortSignal):Promise<string> {
  try{
    let shell="/unknown-shell";
    for(const entry of await client.config.get({location:{directory}},{signal})){
      if(entry.type==="document"&&typeof entry.info?.shell==="string")shell=entry.info.shell;
    }
    return isAbsolute(shell)&&(await stat(shell)).isFile()?shell:"/unknown-shell";
  }catch{return "/unknown-shell";}
}

export default Plugin.define({
  id: "command-guardian",
  async setup(ctx) {
    const missing=missingApis(ctx);
    if(missing.length)throw new Error(`command-guardian: missing OpenCode plugin API: ${missing.join(", ")}`);
    const connection=await connect(ctx.options);
    const client=connection.kind==="connected"?connection.client:undefined;
    // Without a connection there is no guardian approval: block is refused in the shell hook and
    // everything else is left to OpenCode's own permissions instead of stopping the command.
    const misconfigured=()=>{
      if(connection.kind==="failed")console.warn(`command-guardian: could not connect with the configured serverUrl and passwordEnv (${connection.reason}); commands that need approval are left to OpenCode's own permissions.`);
    };
    misconfigured();
    let active=true;
    const local=new AsyncLocalStorage<{readonly context:ToolContext;readonly expected:Invocation|undefined}>();
    const fail=()=>{if(!active)throw new Error("Guardian plugin unloaded.");};
    const force=await ctx.permission.hook("evaluate",event=>{
      if(event.action!=="command-guardian"||event.effect==="deny")return;
      event.effect="ask";
      if(typeof event.metadata?.guardianReason==="string")event.message=event.metadata.guardianReason;
    });
    const approval=client?new Approval({
      newID:()=>Permission.ID.create(),
      events:async function*(signal):AsyncIterable<ApprovalEvent>{
        for await(const event of client.event.subscribe({signal})){
          if(event.type==="server.connected")yield {type:"connected"};
          else if(event.type==="permission.replied")yield {type:"reply",id:event.data.requestID,session:event.data.sessionID,reply:event.data.reply};
        }
      },
      create:async(request,signal)=>{
        const result=await client.permission.create({sessionID:request.session,id:request.id,action:"command-guardian",resources:[request.input.command],save:[],metadata:{guardianReason:request.reason,cwd:request.input.cwd,shell:request.input.shell}}, {signal});
        if(result.id!==request.id)throw new Error("Guardian approval request identity mismatch.");
        return result.effect;
      },
      reject:async(id,session)=>{await ctx.permission.reply({sessionID:session,requestID:id,decision:"reject"});},
    }):undefined;
    const approve=(context:ToolContext)=>async(input:Invocation,reason:string)=>{
      if(approval&&await approval.request(context.sessionID,input,reason,context.signal)==="unavailable")
        console.warn(`command-guardian: could not create the guardian approval request; the command is left to OpenCode's own permissions.`);
    };
    const check=async(input:Invocation,context:ToolContext)=>{
      fail();
      if(context.signal.aborted)throw new Error("Guardian execution cancelled.");
      const result=await judge("command-guardian",input,context.signal);
      await authorize(result,input,approve(context),async()=>{});
      fail();
      if(context.signal.aborted)throw new Error("Guardian execution cancelled.");
    };
    const shellHook=await ctx.shell.hook("create.before",async invocation=>{
      const current=local.getStore();if(!current)return;
      fail();
      const actual=executionInput({command:invocation.command,workdir:invocation.cwd},ctx.location.directory,invocation.shell);
      if(!actual){
        if(!approval)return;
        throw new Error("Could not establish host shell input.");
      }
      const expected=current.expected;
      if(!expected||actual.command!==expected.command||actual.cwd!==expected.cwd||actual.shell!==expected.shell)await check(actual,current.context);
      fail();
      if(current.context.signal.aborted)throw new Error("Guardian execution cancelled.");
    });
    const transform=await ctx.tool.transform(editor=>editor.update("shell",tool=>{
      const original=tool.execute;
      tool.execute=async(input,context)=>{
        try{
          fail();
          const snapshot:unknown=structuredClone(input);
          if(!client){
            misconfigured();
            return await local.run({context,expected:undefined},()=>original(snapshot,context));
          }
          const shell=await configuredShell(client,ctx.location.directory,context.signal);
          const expected=executionInput(snapshot,ctx.location.directory,shell);
          if(!expected)throw new Error("Could not establish guardian execution input.");
          await check(expected,context);
          return await local.run({context,expected},()=>original(snapshot,context));
        }catch(error){
          throw new ToolError({message:error instanceof Error?error.message:"Guardian execution failed."});
        }
      };
    }));
    return async()=>{
      active=false;approval?.close();
      await transform.dispose();await shellHook.dispose();await force.dispose();
    };
  },
});
