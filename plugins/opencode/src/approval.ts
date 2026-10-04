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
  // Waiting starts when the creation response arrives; until then a lost connection does not cancel.
  private readonly pending = new Map<string, {session:string; waiting:boolean; reply:(error:Error|null)=>void; cancel:(error:Error)=>void}>();
  private readonly ready: Promise<void>;
  private connected: (()=>void) = () => {};
  private stopped: Error | undefined;
  private unloaded = false;
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
      this.stop(new Error("Guardian approval connection ended."),false);
    } catch {
      this.stop(new Error("Guardian approval connection failed."),false);
    }
  }
  private reject(id:string,session:string) {
    void this.port.reject(id,session).catch(() => {});
  }
  /** Resolves "approved" after an approval, or "unavailable" when no approval request could be made. */
  async request(session:string,input:Invocation,reason:string,signal:AbortSignal):Promise<"approved"|"unavailable"> {
    if (signal.aborted) throw new Error("Guardian execution cancelled.");
    let rejectReady: (error:Error)=>void = () => {};
    const cancelledReady=new Promise<never>((_resolve,reject)=>{rejectReady=reject;});
    const abortReady=()=>rejectReady(new Error("Guardian execution cancelled."));
    signal.addEventListener("abort",abortReady,{once:true});
    try{
      if(signal.aborted)abortReady();
      await Promise.race([this.ready,cancelledReady]);
    }finally{signal.removeEventListener("abort",abortReady);}
    if (this.unloaded && this.stopped) throw this.stopped;
    if (signal.aborted) throw new Error("Guardian execution cancelled.");
    if (this.stopped) return "unavailable";
    const id = this.port.newID();
    const controller = new AbortController();
    let settle: (error:Error|null)=>void = () => {};
    let cancelled: (error:Error)=>void = () => {};
    // A reply can arrive before the creation response; a rejection received then still stands.
    let settled: {error:Error|null} | undefined;
    const outcome = new Promise<Error|null>(resolve => { settle = error => { settled ??= {error}; resolve(error); }; });
    const cancellation = new Promise<Error>(resolve => { cancelled = resolve; });
    let cancellationError: Error | undefined;
    const cancel = (error:Error) => {
      if (cancellationError) return;
      cancellationError = error;
      controller.abort();
      settle(error); cancelled(error);
      this.reject(id,session);
    };
    const abort = () => cancel(new Error("Guardian execution cancelled."));
    const entry = {session,waiting:false,reply:settle,cancel};
    this.pending.set(id,entry);
    signal.addEventListener("abort",abort,{once:true});
    try {
      if (signal.aborted) abort();
      const creation = this.port.create({id,session,input,reason},controller.signal);
      // A cancelled create may still reach the server. Never resume; retry cleanup when it settles.
      void creation.then(() => { if (cancellationError) this.reject(id,session); }, () => {});
      const created = await Promise.race([creation.then(effect => ({effect}),() => undefined),cancellation]);
      if (created instanceof Error) throw created;
      if (!created) {
        if (cancellationError) throw cancellationError;
        if (settled?.error) throw settled.error;
        // A request whose creation is unknown may remain; any later reply to it is ignored.
        this.reject(id,session);
        return "unavailable";
      }
      const effect = created.effect;
      if (effect !== "ask") throw new Error(`Guardian approval was not requested (${effect}).`);
      entry.waiting = true;
      if (this.stopped) throw this.stopped;
      const rejected = await outcome;
      if (rejected) throw rejected;
      if (cancellationError) throw cancellationError;
      if (this.stopped) throw this.stopped;
      if (signal.aborted) throw new Error("Guardian execution cancelled.");
      return "approved";
    } catch (error) {
      this.reject(id,session);
      throw error;
    } finally {
      signal.removeEventListener("abort",abort);
      this.pending.delete(id);
    }
  }
  close() { this.stop(new Error("Guardian plugin unloaded."),true); }
  private stop(error:Error,unloaded:boolean) {
    if (unloaded) this.unloaded = true;
    this.stopped ??= error;
    this.controller.abort(); this.connected();
    for (const pending of this.pending.values()) if (unloaded || pending.waiting) pending.cancel(this.stopped);
  }
}
