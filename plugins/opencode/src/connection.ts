import { OpenCode } from "@opencode/client";
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

export async function connect(options:Readonly<Record<string,unknown>>) {
  if("serverUrl" in options||"passwordEnv" in options){
    const url=options.serverUrl;
    const password=typeof options.passwordEnv==="string"?process.env[options.passwordEnv]:undefined;
    if(typeof url==="string"&&password)return OpenCode.make({baseUrl:url,headers:{authorization:`Basic ${Buffer.from(`opencode:${password}`).toString("base64")}`}});
    return;
  }
  try{
    const file=join(process.env.XDG_STATE_HOME??join(homedir(),".local","state"),"opencode","service.json");
    const registration:unknown=JSON.parse(await readFile(file,"utf8"));
    if(typeof registration!=="object"||registration===null||!("url" in registration)||typeof registration.url!=="string"||!("password" in registration)||typeof registration.password!=="string")return;
    const endpoint=sameHostEndpoint(registration,{url:registration.url,auth:{type:"basic",username:"opencode",password:registration.password}},process.pid);
    if(!endpoint)return;
    // discover rereads the registration and sends credentials before validating its destination.
    const connection=OpenCode.make({baseUrl:endpoint.url,headers:Service.headers(endpoint),fetch:Object.assign((input:string|URL|Request,init?:RequestInit)=>fetch(input,{...init,redirect:"error"}),fetch)});
    const info=await connection.server.info({signal:AbortSignal.timeout(1000)});
    if(info.pid===process.pid&&info.version==="2.0.21")return connection;
  }catch{return;}
}
