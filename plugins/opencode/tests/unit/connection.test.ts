import { test, expect, spyOn } from "bun:test";
import { connect, sameHostEndpoint } from "../../src/connection.js";
import { mkdtemp, mkdir, writeFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";

const registration={pid:42,version:"2.0.21",url:"http://127.0.0.1:4097",password:"fixture-password-not-real"};
const endpoint={url:registration.url,auth:{type:"basic" as const,username:"opencode",password:registration.password}};

// @kotowari[REQ-052, EX-107, EX-108]
test("automatic_connection_rejects_untrusted_registration_before_sending_credentials",async()=>{
  const root=await mkdtemp(join(tmpdir(),"guardian-connection-"));
  const previous=process.env.XDG_STATE_HOME;
  // Discovery performs authenticated HTTP requests; capture transport to avoid any real destination.
  const transport=spyOn(globalThis,"fetch").mockImplementation(Object.assign(async()=>new Response("{}"),fetch));
  try{
    process.env.XDG_STATE_HOME=root;
    await mkdir(join(root,"opencode"));
    for(const value of [
      {...registration,pid:process.pid,url:"http://example.com:4097"},
      {...registration,pid:process.pid+1},
      {...registration,pid:process.pid,password:undefined},
    ]){
      await writeFile(join(root,"opencode/service.json"),JSON.stringify(value));
      expect(await connect({})).toEqual({kind:"absent"});
    }
    expect(transport.mock.calls.length).toBe(0);
  }finally{
    transport.mockRestore();
    if(previous===undefined)delete process.env.XDG_STATE_HOME;else process.env.XDG_STATE_HOME=previous;
    await rm(root,{recursive:true,force:true});
  }
});

// @kotowari[REQ-052, EX-110]
test("automatic_connection_keeps_the_validated_endpoint_and_does_not_follow_redirects",async()=>{
  const root=await mkdtemp(join(tmpdir(),"guardian-connection-"));
  const previous=process.env.XDG_STATE_HOME;
  const file=join(root,"opencode/service.json");
  const requests:{url:string;redirect:RequestRedirect|undefined}[]=[];
  // Capture HTTP at the transport boundary and simulate registration replacement during the probe.
  const transport=spyOn(globalThis,"fetch").mockImplementation(Object.assign(async(input:string|URL|Request,init?:RequestInit)=>{
    requests.push({url:String(input),redirect:init?.redirect});
    await writeFile(file,JSON.stringify({...registration,pid:process.pid,url:"http://example.com:4097"}));
    return Response.json({version:"2.0.21",pid:process.pid,urls:[registration.url],paths:{tmp:root}});
  },fetch));
  try{
    process.env.XDG_STATE_HOME=root;
    await mkdir(join(root,"opencode"));
    await writeFile(file,JSON.stringify({...registration,pid:process.pid}));
    expect((await connect({})).kind).toBe("connected");
    expect(requests).toEqual([{url:registration.url+"/api/info",redirect:"error"}]);
  }finally{
    transport.mockRestore();
    if(previous===undefined)delete process.env.XDG_STATE_HOME;else process.env.XDG_STATE_HOME=previous;
    await rm(root,{recursive:true,force:true});
  }
});

// @kotowari[REQ-052, EX-106]
test("ex_106_automatic_connection_accepts_its_own_service_whatever_version_it_reports",async()=>{
  const root=await mkdtemp(join(tmpdir(),"guardian-connection-"));
  const previous=process.env.XDG_STATE_HOME;
  // Answer the identity probe at the transport boundary; no real destination is contacted.
  const transport=spyOn(globalThis,"fetch").mockImplementation(Object.assign(async()=>Response.json({version:"9.9.9",pid:process.pid,urls:[registration.url],paths:{tmp:root}}),fetch));
  try{
    process.env.XDG_STATE_HOME=root;
    await mkdir(join(root,"opencode"));
    await writeFile(join(root,"opencode/service.json"),JSON.stringify({...registration,version:"9.9.9",pid:process.pid}));
    expect((await connect({})).kind).toBe("connected");
  }finally{
    transport.mockRestore();
    if(previous===undefined)delete process.env.XDG_STATE_HOME;else process.env.XDG_STATE_HOME=previous;
    await rm(root,{recursive:true,force:true});
  }
});

// @kotowari[REQ-052, EX-106]
test("automatic_connection_accepts_only_the_authenticated_endpoint_of_its_own_process",()=>{
  expect(sameHostEndpoint(registration,endpoint,42)).toEqual(endpoint);
});

// @kotowari[REQ-052, EX-107]
test("automatic_connection_rejects_other_processes_and_changed_registration",()=>{
  expect(sameHostEndpoint(registration,endpoint,43)).toBeUndefined();
  expect(sameHostEndpoint({...registration,url:"http://127.0.0.1:4098"},endpoint,42)).toBeUndefined();
  expect(sameHostEndpoint({...registration,password:"different-fixture-password"},endpoint,42)).toBeUndefined();
});

// @kotowari[REQ-052, EX-108]
test("automatic_connection_rejects_remote_or_unauthenticated_endpoints_and_malformed_registration",()=>{
  for(const value of [null,{},"invalid",{...registration,pid:"42"}])expect(sameHostEndpoint(value,endpoint,42)).toBeUndefined();
  expect(sameHostEndpoint(registration,{url:registration.url},42)).toBeUndefined();
  const remote={...registration,url:"http://example.com:4097"};
  expect(sameHostEndpoint(remote,{...endpoint,url:remote.url},42)).toBeUndefined();
});

// @kotowari[REQ-052, REQ-065]
test("req_052_incomplete_or_rejected_explicit_options_fail_without_automatic_connection",async()=>{
  const root=await mkdtemp(join(tmpdir(),"guardian-connection-"));
  const previous=process.env.XDG_STATE_HOME;
  const requests:string[]=[];
  // The explicit server rejects the credentials; a valid own registration must not be used instead.
  const transport=spyOn(globalThis,"fetch").mockImplementation(Object.assign(async(input:string|URL|Request)=>{requests.push(String(input));return new Response("unauthorized",{status:401});},fetch));
  process.env.GUARDIAN_CONNECTION_TEST_PASSWORD="fixture-password-not-real";
  try{
    process.env.XDG_STATE_HOME=root;
    await mkdir(join(root,"opencode"));
    await writeFile(join(root,"opencode/service.json"),JSON.stringify({...registration,pid:process.pid}));
    expect((await connect({serverUrl:"http://127.0.0.1:4098"})).kind).toBe("failed");
    expect(requests).toEqual([]);
    expect((await connect({serverUrl:"http://127.0.0.1:4098",passwordEnv:"GUARDIAN_CONNECTION_TEST_PASSWORD"})).kind).toBe("failed");
    expect(requests).toEqual(["http://127.0.0.1:4098/api/info"]);
  }finally{
    delete process.env.GUARDIAN_CONNECTION_TEST_PASSWORD;
    transport.mockRestore();
    if(previous===undefined)delete process.env.XDG_STATE_HOME;else process.env.XDG_STATE_HOME=previous;
    await rm(root,{recursive:true,force:true});
  }
});
