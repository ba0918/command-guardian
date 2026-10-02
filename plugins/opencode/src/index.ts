import { Plugin } from "@opencode/plugin";
import { Error as ToolError } from "@opencode/plugin/promise/tool";
import type { ToolContext } from "@opencode/plugin/promise/tool";
import { OpenCode } from "@opencode/client";
import { Permission } from "@opencode/schema/permission";
import { AsyncLocalStorage } from "node:async_hooks";
import { stat } from "node:fs/promises";
import { isAbsolute } from "node:path";
import { Approval, type ApprovalEvent } from "./approval.js";
import { authorize, type Invocation } from "./gate.js";
import { executionInput, judge } from "./guardian.js";

export default Plugin.define({
  id: "command-guardian",
  async setup(ctx) {
    const url=ctx.options.serverUrl;
    const passwordEnv=ctx.options.passwordEnv;
    const password=typeof passwordEnv==="string"?process.env[passwordEnv]:undefined;
    const connection=typeof url==="string"&&typeof password==="string"&&password.length>0?OpenCode.make({baseUrl:url,headers:{authorization:`Basic ${Buffer.from(`opencode:${password}`).toString("base64")}`}}):undefined;
    let active=true;
    const local=new AsyncLocalStorage<{readonly context:ToolContext;readonly expected:Invocation|undefined}>();
    const fail=()=>{if(!active)throw new Error("Guardian plugin unloaded.");};
    const force=await ctx.permission.hook("evaluate",event=>{
      if(event.action!=="command-guardian"||event.effect==="deny")return;
      event.effect="ask";
      if(typeof event.metadata?.guardianReason==="string")event.message=event.metadata.guardianReason;
    });
    const approval=connection?new Approval({
      newID:()=>Permission.ID.create(),
      events:async function*(signal):AsyncIterable<ApprovalEvent>{
        for await(const event of connection.event.subscribe({signal})){
          if(event.type==="server.connected")yield {type:"connected"};
          else if(event.type==="permission.replied")yield {type:"reply",id:event.data.requestID,session:event.data.sessionID,reply:event.data.reply};
        }
      },
      create:async(request,signal)=>{
        const result=await connection.permission.create({sessionID:request.session,id:request.id,action:"command-guardian",resources:[request.input.command],save:[],metadata:{guardianReason:request.reason,cwd:request.input.cwd,shell:request.input.shell}}, {signal});
        if(result.id!==request.id)throw new Error("Guardian approval request identity mismatch.");
        return result.effect;
      },
      reject:async(id,session)=>{await ctx.permission.reply({sessionID:session,requestID:id,decision:"reject"});},
    }):undefined;
    const approve=(context:ToolContext)=>(input:Invocation,reason:string)=>{
      if(!approval)throw new Error("Guardian approval connection is not configured. Set serverUrl and passwordEnv for this server.");
      return approval.request(context.sessionID,input,reason,context.signal);
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
      if(!actual)throw new Error("Could not establish host shell input.");
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
          if(ctx.app.version!=="2.0.21")throw new Error("Guardian plugin requires verified OpenCode V2 2.0.21.");
          const snapshot:unknown=structuredClone(input);
          let shell="/unknown-shell";
          if(connection){
            try{
              const entries=await connection.config.get({location:{directory:ctx.location.directory}},{signal:context.signal});
              for(const entry of entries){
                if(entry.type==="document"&&typeof entry.info?.shell==="string")shell=entry.info.shell;
              }
              if(!isAbsolute(shell)||!(await stat(shell)).isFile())shell="/unknown-shell";
            }catch{shell="/unknown-shell";}
          }
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
