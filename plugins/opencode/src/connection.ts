import { OpenCode, type OpenCodeClient } from "@opencode/client";
import { Service, type Endpoint } from "@opencode/client/service";
import { readFile } from "node:fs/promises";
import { homedir } from "node:os";
import { join } from "node:path";

type DiscoveredEndpoint={readonly url:string;readonly auth?:Endpoint["auth"]|undefined};

export function sameHostEndpoint(registration:unknown,endpoint:DiscoveredEndpoint|undefined,pid:number):Endpoint|undefined {
  if(typeof registration!=="object"||registration===null||!endpoint?.auth)return;
  if(!("pid" in registration)||registration.pid!==pid||!("url" in registration)||registration.url!==endpoint.url)return;
  if(!("password" in registration)||registration.password!==endpoint.auth.password||!endpoint.auth.password||endpoint.auth.username!=="opencode")return;
  try{
    const url=new URL(endpoint.url);
    if(url.protocol!=="http:"||!["127.0.0.1","[::1]","localhost"].includes(url.hostname)||url.username||url.password)return;
    return {url:endpoint.url,auth:endpoint.auth};
  }catch{return;}
}

export type Connection=
  | {readonly kind:"connected";readonly client:OpenCodeClient}
  // No connection options and no verified own managed service.
  | {readonly kind:"absent"}
  // Explicit connection options that are incomplete or were refused.
  | {readonly kind:"failed";readonly reason:string};

async function explicit(options:Readonly<Record<string,unknown>>):Promise<Connection> {
  const url=options.serverUrl;
  const password=typeof options.passwordEnv==="string"?process.env[options.passwordEnv]:undefined;
  if(typeof url!=="string"||!password)return {kind:"failed",reason:"serverUrl or the variable named by passwordEnv is not set"};
  try{
    const client=OpenCode.make({baseUrl:url,headers:{authorization:`Basic ${Buffer.from(`opencode:${password}`).toString("base64")}`}});
    await client.server.info({signal:AbortSignal.timeout(1000)});
    return {kind:"connected",client};
  }catch(error){
    return {kind:"failed",reason:error instanceof Error?error.message:"the server could not be reached"};
  }
}

export async function connect(options:Readonly<Record<string,unknown>>):Promise<Connection> {
  if("serverUrl" in options||"passwordEnv" in options)return explicit(options);
  try{
    const file=join(process.env.XDG_STATE_HOME??join(homedir(),".local","state"),"opencode","service.json");
    const registration:unknown=JSON.parse(await readFile(file,"utf8"));
    if(typeof registration!=="object"||registration===null||!("url" in registration)||typeof registration.url!=="string"||!("password" in registration)||typeof registration.password!=="string")return {kind:"absent"};
    const endpoint=sameHostEndpoint(registration,{url:registration.url,auth:{type:"basic",username:"opencode",password:registration.password}},process.pid);
    if(!endpoint)return {kind:"absent"};
    // discover rereads the registration and sends credentials before validating its destination.
    const client=OpenCode.make({baseUrl:endpoint.url,headers:Service.headers(endpoint),fetch:Object.assign((input:string|URL|Request,init?:RequestInit)=>fetch(input,{...init,redirect:"error"}),fetch)});
    const info=await client.server.info({signal:AbortSignal.timeout(1000)});
    if(info.pid===process.pid)return {kind:"connected",client};
  }catch{}
  return {kind:"absent"};
}
