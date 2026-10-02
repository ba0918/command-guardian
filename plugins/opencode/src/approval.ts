import type { Invocation } from "./gate.js";
export type ApprovalEvent = {type:"connected"} | {type:"reply";id:string;session:string;reply:"once"|"always"|"reject"};
export interface ApprovalRequest {readonly id:string;readonly session:string;readonly input:Invocation;readonly reason:string;}
export interface ApprovalPort {
  newID():string;
  events(signal:AbortSignal):AsyncIterable<ApprovalEvent>;
  create(request:ApprovalRequest,signal:AbortSignal):Promise<"ask"|"allow"|"deny">;
  reject(id:string,session:string):Promise<void>;
}
export class Approval {
  private readonly controller = new AbortController();
  private readonly pending = new Map<string, {session:string; reply:(error:Error|null)=>void; cancel:(error:Error)=>void}>();
  private readonly ready: Promise<void>;
  private connected: (()=>void) = () => {};
  private stopped: Error | undefined;
  constructor(private readonly port:ApprovalPort) {
    this.ready = new Promise(resolve => { this.connected = resolve; });
    void this.listen();
  }
  private async listen() {
    try {
      for await (const event of this.port.events(this.controller.signal)) {
        if (this.stopped) break;
        if (event.type === "connected") this.connected();
        else {
          const pending = this.pending.get(event.id);
          if (pending?.session === event.session) pending.reply(event.reply === "reject" ? new Error("Guardian approval rejected.") : null);
        }
      }
      this.close(new Error("Guardian approval connection ended."));
    } catch {
      this.close(new Error("Guardian approval connection failed."));
    }
  }
  private reject(id:string,session:string) {
    void this.port.reject(id,session).catch(() => {});
  }
  async request(session:string,input:Invocation,reason:string,signal:AbortSignal):Promise<void> {
    if (signal.aborted) throw new Error("Guardian execution cancelled.");
    let rejectReady: (error:Error)=>void = () => {};
    const cancelledReady=new Promise<never>((_resolve,reject)=>{rejectReady=reject;});
    const abortReady=()=>rejectReady(new Error("Guardian execution cancelled."));
    signal.addEventListener("abort",abortReady,{once:true});
    try{
      if(signal.aborted)abortReady();
      await Promise.race([this.ready,cancelledReady]);
    }finally{signal.removeEventListener("abort",abortReady);}
    if (this.stopped) throw this.stopped;
    if (signal.aborted) throw new Error("Guardian execution cancelled.");
    const id = this.port.newID();
    const controller = new AbortController();
    let settle: (error:Error|null)=>void = () => {};
    let cancelled: (error:Error)=>void = () => {};
    const outcome = new Promise<Error|null>(resolve => { settle = resolve; });
    const cancellation = new Promise<Error>(resolve => { cancelled = resolve; });
    let cancellationError: Error | undefined;
    const cancel = (error:Error) => {
      cancellationError ??= error;
      controller.abort();
      settle(error); cancelled(error);
      this.reject(id,session);
    };
    const abort = () => cancel(new Error("Guardian execution cancelled."));
    this.pending.set(id,{session,reply:settle,cancel});
    signal.addEventListener("abort",abort,{once:true});
    try {
      if (signal.aborted) abort();
      const creation = this.port.create({id,session,input,reason},controller.signal);
      // A cancelled create may still reach the server. Never resume; retry cleanup when it settles.
      void creation.then(() => { if (cancellationError) this.reject(id,session); }, () => {});
      const effect = await Promise.race([creation,cancellation]);
      if (effect instanceof Error) throw effect;
      if (effect !== "ask") throw new Error(`Guardian approval was not requested (${effect}).`);
      const rejected = await outcome;
      if (rejected) throw rejected;
      if (cancellationError) throw cancellationError;
      if (this.stopped) throw this.stopped;
      if (signal.aborted) throw new Error("Guardian execution cancelled.");
    } catch (error) {
      this.reject(id,session);
      throw error;
    } finally {
      signal.removeEventListener("abort",abort);
      this.pending.delete(id);
    }
  }
  close(error = new Error("Guardian plugin unloaded.")) {
    if (this.stopped) return;
    this.stopped = error;
    this.controller.abort(); this.connected();
    for (const pending of this.pending.values()) pending.cancel(error);
  }
}
