import { test, expect } from "bun:test";
import { OpenCode } from "@opencode/client";
import { Service } from "@opencode/client/service";
import { Permission } from "@opencode/schema/permission";
import { spawn } from "node:child_process";
import { mkdtemp, mkdir, writeFile, readFile, symlink, rm, appendFile } from "node:fs/promises";
import { resolve, join } from "node:path";
import { tmpdir } from "node:os";
import { stopHost } from "../helpers/host-process";

async function host(options:{enforce?:boolean;shell?:string;nativeDeny?:boolean;wrongAuth?:boolean;managedService?:boolean;incompleteConnection?:boolean;controlPeer?:boolean;advisor?:boolean}={}) {
  const binary=process.env.OPENCODE_TEST_BIN,guardian=process.env.GUARDIAN_TEST_BIN;
  if (!binary || !guardian) throw new Error("Explicit OPENCODE_TEST_BIN and GUARDIAN_TEST_BIN are required");
  const version=Bun.spawnSync([binary,"--version"]);
  if (!version.success || version.stdout.toString().trim()!=="opencode v2.0.21") throw new Error("Expected OpenCode v2.0.21");
  const root=await mkdtemp(join(resolve(import.meta.dir,"../../../../target"),"native-guardian-"));
  const home=join(root,"home"),project=join(home,"project"),bin=join(root,"bin");
  await mkdir(project,{recursive:true}); await mkdir(bin);
  if(options.controlPeer){
    await writeFile(join(bin,"command-guardian"),`#!/usr/bin/python3
import socket,json,time,sys
sys.stdin.read()
s=socket.socket(fileno=3)
n='07070707070707070707070707070707'
def send(x):s.sendall((json.dumps(x)+'\\n').encode())
def receive():
 b=b''
 while not b.endswith(b'\\n'):b+=s.recv(1)
 return json.loads(b)
send({'version':1,'nonce':n,'kind':'budget_probe'})
r=receive()
assert r['kind']=='budget_reply' and r['original_remaining_ms']>0
send({'version':1,'nonce':n,'kind':'advisory_start','timeout_ms':10000})
r=receive()
assert r['kind']=='advisory_ack' and r['accepted']
time.sleep(6.1)
print(json.dumps({'status':'judged','mode':{'enforce':True},'verdict':'allow','reason':'fixture control completed'}))
`,{mode:0o700});
  }else await symlink(guardian,join(bin,"command-guardian"));
  const config=join(home,"config","command-guardian");await mkdir(config,{recursive:true});
  await writeFile(join(config,"config.toml"),`[mode]\nenforce = ${options.enforce??true}\n[paths]\nprotected_roots = [${JSON.stringify(join(project,"sentinel"))}]\n${options.advisor ? "[advisor]\nmode='enforce'\ntimeout_ms=10000\n" : ""}`);
  const sentinel=join(project,"sentinel");await writeFile(sentinel,"unchanged");
  // Hold a loopback port only long enough to select it; authenticate every subsequent request.
  const listener=Bun.serve({hostname:"127.0.0.1",port:0,fetch:()=>new Response("fixture")});
  const port=listener.port;listener.stop(true);
  const url=`http://127.0.0.1:${port}`;
  const environment:NodeJS.ProcessEnv={};
  for (const [key,value] of Object.entries(process.env)) if(!key.startsWith("OPENCODE")&&!key.startsWith("GIT_")&&!key.startsWith("SAFE_CHAIN_MINIMUM"))environment[key]=value;
  delete environment.TYPESAFE_API_KEY;
  Object.assign(environment,{HOME:home,XDG_CONFIG_HOME:join(home,"config"),XDG_STATE_HOME:join(home,"state"),XDG_CACHE_HOME:join(home,"cache"),XDG_DATA_HOME:join(home,"data"),TMPDIR:tmpdir(),PATH:`${bin}:${process.env.PATH}`,OPENCODE_DB:join(home,"fixture.db"),OPENCODE_SERVER_PASSWORD:"guardian-test-password-not-real",GUARDIAN_TEST_PASSWORD:options.wrongAuth?"incorrect-fixture-password":"guardian-test-password-not-real",OPENCODE_DISABLE_PROJECT_CONFIG:"1",OPENCODE_DISABLE_MODELS_FETCH:"1",OPENCODE_CONFIG_CONTENT:JSON.stringify({shell:options.shell??"/bin/bash",permissions:[{action:"*",resource:"*",effect:options.nativeDeny?"deny":"allow"}],plugins:[{package:process.env.GUARDIAN_PLUGIN_DIR??resolve(import.meta.dir,"../.."),options:{serverUrl:url,passwordEnv:"GUARDIAN_TEST_PASSWORD"}},process.env.GUARDIAN_TEST_BRIDGE_DIR??resolve(import.meta.dir,"bridge")]})});
  let logs="";
  if(options.managedService){
    const config=JSON.parse(environment.OPENCODE_CONFIG_CONTENT!);
    if(!options.wrongAuth)delete config.plugins[0].options;
    if(options.incompleteConnection)config.plugins[0].options={serverUrl:url};
    environment.OPENCODE_CONFIG_CONTENT=JSON.stringify(config);
  }
  const started=performance.now();
  let spawnError:Error|undefined,readyMs:number|undefined;
  const server=spawn(binary,["serve","--hostname","127.0.0.1","--port",String(port),...(options.managedService?["--service"]:[])],{cwd:project,env:environment,stdio:["ignore","pipe","pipe"]});
  server.on("error",error=>{spawnError=error;});
  server.stdout.on("data",data=>{logs+=String(data);});server.stderr.on("data",data=>{logs+=String(data);});
  const close=async(outcome:string)=>{
    const beforeCleanup={elapsedMs:performance.now()-started,readyMs:readyMs??null,exitCode:server.exitCode,signalCode:server.signalCode,spawnError:spawnError?.message??null};
    try { await stopHost(server); }
    finally {
      try {
        const diagnostics=await readFile(join(home,"data/opencode/log/opencode.log"),"utf8").catch(()=>"");
        const selected=diagnostics.split("\n").filter(line=>line.includes("cli starting")||line.includes("database schema bootstrap")||line.includes("plugin")||line.includes("level=WARN")||line.includes("level=ERROR")).join("\n");
        const safe=JSON.stringify({outcome,beforeCleanup,afterCleanup:{exitCode:server.exitCode,signalCode:server.signalCode},logs,diagnostics:selected})
          .replaceAll(root,"<fixture>").replaceAll(resolve(import.meta.dir,"../../../.."),"<worktree>")
          .replaceAll(process.env.HOME??home,"<runner-home>")
          .replaceAll("guardian-test-password-not-real","<fixture-password>")
          .replaceAll(Buffer.from("opencode:guardian-test-password-not-real").toString("base64"),"<fixture-auth>");
        const artifacts="/tmp/opencode/llm-advisor-host-readiness";
        await mkdir(artifacts,{recursive:true});
        await appendFile(join(artifacts,"fixtures.jsonl"),safe+"\n");
      } finally { await rm(root,{recursive:true,force:true}); }
    }
  };
  let client=OpenCode.make({baseUrl:url,headers:{authorization:`Basic ${Buffer.from("opencode:guardian-test-password-not-real").toString("base64")}`}});
  try {
    for(let i=0;i<100;i++) {
      if(spawnError)throw new Error("Isolated server spawn failed",{cause:spawnError});
      if(server.exitCode!==null||server.signalCode!==null)throw new Error("Isolated server exited");
      if(logs.includes(`server listening on ${url}`)){readyMs=performance.now()-started;break;}
      await new Promise(resolve=>setTimeout(resolve,50));
    }
    if(!logs.includes(`server listening on ${url}`))throw new Error("Own server readiness missing");
    if(options.managedService){
      const endpoint=await Service.discover({file:join(home,"state/opencode/service.json"),version:"2.0.21"});
      if(!endpoint||endpoint.url!==url)throw new Error("Own managed service registration missing");
      client=OpenCode.make({baseUrl:endpoint.url,headers:Service.headers({url:endpoint.url,...(endpoint.auth?{auth:endpoint.auth}:{})})});
    }
    const info=await client.server.info();expect(info.version).toBe("2.0.21");
    const session=await client.session.create({title:"model-free guardian fixture",location:{directory:project}});
    return {root,project,sentinel,client,session,execute:(command:string,extra:{workdir?:string;background?:boolean;codeMode?:boolean}={},signal?:AbortSignal)=>client.rpc.call({rpcID:"guardian-test",method:"execute",location:{directory:project},input:{session:session.id,command,...extra}},signal?{signal}:{}),close:async()=>{
      await close("closed");
    }};
  }catch(error){await close(error instanceof Error?error.message:"setup failed");throw error;}
}

// @kotowari[REQ-advisor-020, REQ-advisor-021, REQ-advisor-011, EX-advisor-024, EX-advisor-042]
test("fixed_host_real_guardian_negotiates_then_missing_auth_preserves_safe_execution_and_metadata_privacy",async()=>{
  for(const enforce of [true,false]){
    const f=await host({advisor:true,enforce});
    try{
      await f.execute("printf fixture-enabled-private");
      const text=await readFile(join(f.root,"home/state/command-guardian/advisor.jsonl"),"utf8");
      const log=JSON.parse(text.trim());
      expect(log.failure).toBe("authentication");expect(log.final).toBe("allow");
      expect(text).not.toContain("fixture-enabled-private");expect(text).not.toContain(f.session.id);
      expect(await f.client.permission.list({sessionID:f.session.id})).toHaveLength(0);
      expect(await readFile(f.sentinel,"utf8")).toBe("unchanged");
    }finally{await f.close();}
  }
},20000);

// @kotowari[REQ-advisor-020, EX-advisor-039]
test("fixed_host_passes_duplex_control_fd_and_accepts_result_after_original_six_seconds",async()=>{
  const f=await host({controlPeer:true});
  try{
    const started=performance.now();
    const result=await f.execute("printf fixture-control > control-output");
    expect(performance.now()-started).toBeGreaterThan(6000);
    expect(await readFile(join(f.project,"control-output"),"utf8")).toBe("fixture-control");
    expect(await f.client.permission.list({sessionID:f.session.id})).toHaveLength(0);
    expect(JSON.stringify(result)).not.toContain("timed out");
    expect(await readFile(f.sentinel,"utf8")).toBe("unchanged");
  }finally{await f.close();}
},20000);

// @kotowari[REQ-047, REQ-051, EX-103]
test("req_047_host_valid_empty_command_waits_for_fresh_approval_and_honors_reply",async()=>{
  const f=await host();
  try{
    for(const decision of ["once","reject"] as const){
      let completed=false;
      const run=f.execute("",{workdir:f.project}).then(result=>{completed=true;return result;});
      let requests=await f.client.permission.list({sessionID:f.session.id});
      for(let i=0;i<100&&!requests.length&&!completed;i++){
        await new Promise(resolve=>setTimeout(resolve,20));
        requests=await f.client.permission.list({sessionID:f.session.id});
      }
      expect(requests).toHaveLength(1);
      expect(completed).toBe(false);
      const request=requests[0];if(!request)throw new Error("Missing input-uncertainty approval");
      expect(request.resources).toEqual([""]);
      expect(request.metadata?.cwd).toBe(f.project);
      expect(request.metadata?.shell).toBe("/bin/bash");
      expect(request.metadata?.guardianReason).toBeString();
      expect(String(request.metadata?.guardianReason).length).toBeGreaterThan(0);
      await f.client.permission.reply({sessionID:f.session.id,requestID:request.id,decision});
      const result=await run;
      expect(completed).toBe(true);
      expect(typeof result.output).toBe("object");
      if(typeof result.output!=="object"||result.output===null)throw new Error("Missing host executor result");
      if(decision==="once"){
        expect("error" in result.output).toBe(false);
        expect("content" in result.output).toBe(true);
      }else expect("error" in result.output).toBe(true);
      expect(await readFile(f.sentinel,"utf8")).toBe("unchanged");
      expect(await f.client.permission.list({sessionID:f.session.id})).toHaveLength(0);
    }
  }finally{await f.close();}
},20000);

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

// @kotowari[REQ-052, EX-106]
test("managed_service_without_connection_options_preserves_allow_block_and_native_approval",async()=>{
  const f=await host({managedService:true});
  try{
    const allowed=await f.execute("printf guardian-automatic-connection");
    expect(JSON.stringify(allowed)).toContain("guardian-automatic-connection");
    const blocked=await f.execute("> sentinel");
    expect(JSON.stringify(blocked)).toContain("protected");
    expect(await readFile(f.sentinel,"utf8")).toBe("unchanged");
    for(const decision of ["once","reject"] as const){
      const run=f.execute("").then(value=>({value}),error=>({error:String(error)}));
      const requests=await pending(f,1);
      if(!requests[0])throw new Error("Missing automatic connection approval");
      expect(requests[0].metadata?.shell).toBe("/bin/bash");
      await f.client.permission.reply({sessionID:f.session.id,requestID:requests[0].id,decision});
      const result=await run;
      expect("error" in result).toBe(false);
      if("value" in result)expect(typeof result.value.output==="object"&&result.value.output!==null&&"error" in result.value.output).toBe(decision==="reject");
    }
  }finally{await f.close();}
},20000);

// @kotowari[REQ-052, EX-109]
test("managed_service_does_not_replace_incomplete_explicit_connection_options",async()=>{
  const f=await host({managedService:true,incompleteConnection:true});
  try{
    const result=await f.execute("printf ran > connection-failure");
    expect(JSON.stringify(result)).toContain("connect");
    await expect(readFile(join(f.project,"connection-failure"),"utf8")).rejects.toThrow();
  }finally{await f.close();}
},20000);

// @kotowari[REQ-052, EX-109]
test("managed_service_does_not_replace_failed_explicit_authentication",async()=>{
  const f=await host({managedService:true,wrongAuth:true});
  try{
    const result=await f.execute("printf ran > connection-failure");
    expect(JSON.stringify(result)).toContain("connection");
    await expect(readFile(join(f.project,"connection-failure"),"utf8")).rejects.toThrow();
  }finally{await f.close();}
},20000);

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
