import { test, expect } from "bun:test";
import { OpenCode } from "@opencode/client";
import { Permission } from "@opencode/schema/permission";
import { spawn } from "node:child_process";
import { mkdtemp, mkdir, writeFile, readFile, symlink, rm } from "node:fs/promises";
import { resolve, join } from "node:path";
import { tmpdir } from "node:os";

async function host(options:{enforce?:boolean;shell?:string;nativeDeny?:boolean;wrongAuth?:boolean}={}) {
  const binary=process.env.OPENCODE_TEST_BIN,guardian=process.env.GUARDIAN_TEST_BIN;
  if (!binary || !guardian) throw new Error("Explicit OPENCODE_TEST_BIN and GUARDIAN_TEST_BIN are required");
  const version=Bun.spawnSync([binary,"--version"]);
  if (!version.success || version.stdout.toString().trim()!=="opencode v2.0.21") throw new Error("Expected OpenCode v2.0.21");
  const root=await mkdtemp(join(resolve(import.meta.dir,"../../../../target"),"native-guardian-"));
  const home=join(root,"home"),project=join(home,"project"),bin=join(root,"bin");
  await mkdir(project,{recursive:true}); await mkdir(bin);
  await symlink(guardian,join(bin,"command-guardian"));
  const config=join(home,"config","command-guardian");await mkdir(config,{recursive:true});
  await writeFile(join(config,"config.toml"),`[mode]\nenforce = ${options.enforce??true}\n[paths]\nprotected_roots = [${JSON.stringify(join(project,"sentinel"))}]\n`);
  const sentinel=join(project,"sentinel");await writeFile(sentinel,"unchanged");
  // Hold a loopback port only long enough to select it; authenticate every subsequent request.
  const listener=Bun.serve({hostname:"127.0.0.1",port:0,fetch:()=>new Response("fixture")});
  const port=listener.port;listener.stop(true);
  const url=`http://127.0.0.1:${port}`;
  const environment:NodeJS.ProcessEnv={};
  for (const [key,value] of Object.entries(process.env)) if(!key.startsWith("OPENCODE")&&!key.startsWith("GIT_")&&!key.startsWith("SAFE_CHAIN_MINIMUM"))environment[key]=value;
  Object.assign(environment,{HOME:home,XDG_CONFIG_HOME:join(home,"config"),XDG_STATE_HOME:join(home,"state"),XDG_CACHE_HOME:join(home,"cache"),XDG_DATA_HOME:join(home,"data"),TMPDIR:tmpdir(),PATH:`${bin}:${process.env.PATH}`,OPENCODE_DB:join(home,"fixture.db"),OPENCODE_SERVER_PASSWORD:"guardian-test-password-not-real",GUARDIAN_TEST_PASSWORD:options.wrongAuth?"incorrect-fixture-password":"guardian-test-password-not-real",OPENCODE_DISABLE_PROJECT_CONFIG:"1",OPENCODE_DISABLE_MODELS_FETCH:"1",OPENCODE_CONFIG_CONTENT:JSON.stringify({shell:options.shell??"/bin/bash",permissions:[{action:"*",resource:"*",effect:options.nativeDeny?"deny":"allow"}],plugins:[{package:process.env.GUARDIAN_PLUGIN_DIR??resolve(import.meta.dir,"../.."),options:{serverUrl:url,passwordEnv:"GUARDIAN_TEST_PASSWORD"}},process.env.GUARDIAN_TEST_BRIDGE_DIR??resolve(import.meta.dir,"bridge")]})});
  let logs="";
  const server=spawn(binary,["serve","--hostname","127.0.0.1","--port",String(port)],{cwd:project,env:environment,stdio:["ignore","pipe","pipe"]});
  server.stdout.on("data",data=>{logs+=String(data);});server.stderr.on("data",data=>{logs+=String(data);});
  const client=OpenCode.make({baseUrl:url,headers:{authorization:`Basic ${Buffer.from("opencode:guardian-test-password-not-real").toString("base64")}`}});
  try {
    for(let i=0;i<100;i++) {
      if(server.exitCode!==null)throw new Error("Isolated server exited");
      if(logs.includes(`server listening on ${url}`))break;
      await new Promise(resolve=>setTimeout(resolve,50));
    }
    if(!logs.includes(`server listening on ${url}`))throw new Error("Own server readiness missing");
    const info=await client.server.info();expect(info.version).toBe("2.0.21");
    const session=await client.session.create({title:"model-free guardian fixture"});
    return {root,project,sentinel,client,session,execute:(command:string,extra:{workdir?:string;background?:boolean;codeMode?:boolean}={},signal?:AbortSignal)=>client.rpc.call({rpcID:"guardian-test",method:"execute",input:{session:session.id,command,...extra}},signal?{signal}:{}),close:async()=>{
      server.kill("SIGTERM");await new Promise<void>(resolve=>server.once("exit",()=>resolve()));
      const diagnostics=await readFile(join(home,"data/opencode/log/opencode.log"),"utf8").catch(()=>"");
      const safe=(diagnostics.split("\n").filter(line=>line.includes("plugin")||line.includes("level=WARN")||line.includes("level=ERROR")).join("\n")+"\n"+logs).replaceAll(root,"<fixture>").replaceAll(resolve(import.meta.dir,"../../../.."),"<worktree>");
      await writeFile(resolve(import.meta.dir,"../../../../.agents/artifacts/native-fixture-diagnostics.log"),safe);
      await rm(root,{recursive:true,force:true});
    }};
  }catch(error){server.kill("SIGTERM");throw error;}
}

// @kotowari[REQ-049, EX-083]
test("req_049_real_host_registered_plugin_blocks_fixture_truncation_even_with_native_allow",async()=>{
  const f=await host();
  try {
    const result=await f.execute("> sentinel");
    expect(await readFile(f.sentinel,"utf8")).toBe("unchanged");
    expect(JSON.stringify(result)).toContain("protected");
  }finally{await f.close();}
},20000);

// @kotowari[REQ-048, REQ-049, EX-080, EX-082]
test("req_048_native_shadow_preserves_host_denial_and_does_not_enforce_guardian_block",async()=>{
  for(const nativeDeny of [false,true]){
    const f=await host({enforce:false,nativeDeny});
    try{
      await f.execute("printf ran > sentinel");
      expect(await readFile(f.sentinel,"utf8")).toBe(nativeDeny?"unchanged":"ran");
      expect(await f.client.permission.list({sessionID:f.session.id})).toHaveLength(0);
    }finally{await f.close();}
  }
},20000);

// @kotowari[REQ-055, EX-094, EX-095]
test("req_055_native_nonbash_waits_for_approval_instead_of_becoming_allow",async()=>{
  const f=await host({shell:"/bin/dash"});
  try{
    const run=f.execute("printf ran > nonbash");
    const requests=await pending(f,1);if(!requests[0])throw new Error("Missing nonbash request");
    expect(requests[0].metadata?.guardianReason).toContain("Unsupported shell");
    await expect(readFile(join(f.project,"nonbash"),"utf8")).rejects.toThrow();
    await f.client.permission.reply({sessionID:f.session.id,requestID:requests[0].id,decision:"once"});
    await run;expect(await readFile(join(f.project,"nonbash"),"utf8")).toBe("ran");
  }finally{await f.close();}
},20000);

// @kotowari[REQ-052, EX-089]
test("req_052_native_authentication_failure_does_not_start_the_command",async()=>{
  const f=await host({wrongAuth:true});
  try{
    const result=await f.execute('printf ran > auth-failure; eval "$GUARDIAN_FIXTURE_UNKNOWN"');
    await expect(readFile(join(f.project,"auth-failure"),"utf8")).rejects.toThrow();
    expect(JSON.stringify(result)).toContain("connection");
  }finally{await f.close();}
},20000);

// @kotowari[REQ-047, EX-078]
test("req_047_background_and_codemode_use_the_host_registered_guardian_executor",async()=>{
  const f=await host();
  try{
    for(const route of [{background:true},{codeMode:true}]){
      const result=await f.execute("> sentinel",route);
      expect(await readFile(f.sentinel,"utf8")).toBe("unchanged");
      expect(JSON.stringify(result)).toContain("protected");
    }
  }finally{await f.close();}
},20000);


// @kotowari[REQ-050, REQ-052, EX-084, EX-085, EX-088]
test("req_050_native_allow_and_saved_permission_do_not_skip_guardian_request",async()=>{
  const f=await host();
  try{
    await f.execute("true");
    const saved=await f.client.permission.create({sessionID:f.session.id,id:Permission.ID.create(),agent:"build",action:"command-guardian",resources:["fixture"],save:["*"]});
    expect(saved.effect).toBe("ask");
    await f.client.permission.reply({sessionID:f.session.id,requestID:saved.id,decision:"always"});
    expect((await f.client.permission.saved.list()).some(rule=>rule.action==="command-guardian")).toBe(true);
    for(const reply of ["always","once"] as const){
      const output=join(f.project,`approved-${reply}`);
      let finished=false;
      const run=f.execute(`printf ran > approved-${reply}; eval "$GUARDIAN_FIXTURE_UNKNOWN"`).then(value=>{finished=true;return value;});
      let requests=await f.client.permission.list({sessionID:f.session.id});
      for(let i=0;i<50&&!requests.length;i++){await new Promise(resolve=>setTimeout(resolve,20));requests=await f.client.permission.list({sessionID:f.session.id});}
      expect(requests).toHaveLength(1);expect(finished).toBe(false);
      await expect(readFile(output,"utf8")).rejects.toThrow();
      const request=requests[0];if(!request)throw new Error("Native request missing");
      expect(request.metadata?.guardianReason).toBeString();
      await f.client.permission.reply({sessionID:f.session.id,requestID:request.id,decision:reply});
      await run;expect(await readFile(output,"utf8")).toBe("ran");
    }
  }finally{await f.close();}
},20000);

async function pending(f:Awaited<ReturnType<typeof host>>,count:number){
  for(let i=0;i<100;i++){
    const requests=await f.client.permission.list({sessionID:f.session.id});
    if(requests.length===count)return requests;
    await new Promise(resolve=>setTimeout(resolve,20));
  }
  throw new Error("Native approval request count did not arrive");
}

// @kotowari[REQ-050, REQ-054, EX-092, EX-102]
test("req_054_native_parallel_requests_preserve_cwd_and_rejection_batch",async()=>{
  const f=await host();
  try{
    await mkdir(join(f.project,"second"));
    const command='printf ran > result; eval "$GUARDIAN_FIXTURE_UNKNOWN"';
    let secondDone=false;
    const a=f.execute(command), b=f.execute(command,{workdir:"second"}).then(result=>{secondDone=true;return result;});
    const requests=await pending(f,2);
    const first=requests.find(request=>request.metadata?.cwd===f.project);
    const second=requests.find(request=>request.metadata?.cwd===join(f.project,"second"));
    if(!first||!second)throw new Error("Native cwd identities missing");
    expect(first.id).not.toBe(second.id);
    await f.client.permission.reply({sessionID:f.session.id,requestID:first.id,decision:"once"});
    await a;expect(await readFile(join(f.project,"result"),"utf8")).toBe("ran");expect(secondDone).toBe(false);
    await f.client.permission.reply({sessionID:f.session.id,requestID:second.id,decision:"reject"});
    await b;await expect(readFile(join(f.project,"second/result"),"utf8")).rejects.toThrow();
    const c=f.execute(command,{workdir:"second"}),d=f.execute(command,{workdir:"second"});
    const batch=await pending(f,2);if(!batch[0])throw new Error("Missing rejection request");
    await f.client.permission.reply({sessionID:f.session.id,requestID:batch[0].id,decision:"reject"});
    await Promise.all([c,d]);expect(await f.client.permission.list({sessionID:f.session.id})).toHaveLength(0);
    await expect(readFile(join(f.project,"second/result"),"utf8")).rejects.toThrow();
  }finally{await f.close();}
},20000);

// @kotowari[REQ-053, EX-091, EX-093]
test("req_053_cancelled_native_executor_does_not_run_on_late_reply",async()=>{
  const f=await host();
  try{
    const controller=new AbortController();
    const run=f.execute('printf ran > cancelled; eval "$GUARDIAN_FIXTURE_UNKNOWN"',{},controller.signal).then(value=>value,error=>({error:String(error)}));
    const requests=await pending(f,1);if(!requests[0])throw new Error("Missing cancel request");
    controller.abort();await run;
    await new Promise(resolve=>setTimeout(resolve,50));
    const retained=await f.client.permission.list({sessionID:f.session.id});
    if(retained.some(request=>request.id===requests[0]?.id))await f.client.permission.reply({sessionID:f.session.id,requestID:requests[0].id,decision:"once"});
    await new Promise(resolve=>setTimeout(resolve,50));
    await expect(readFile(join(f.project,"cancelled"),"utf8")).rejects.toThrow();
  }finally{await f.close();}
},20000);
