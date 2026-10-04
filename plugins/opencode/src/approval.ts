import type { Invocation } from "./gate.js";
export type ApprovalEvent = {type:"connected"} | {type:"reply";id:string;session:string;reply:"once"|"always"|"reject"};
export interface ApprovalRequest {readonly id:string;readonly session:string;readonly input:Invocation;readonly reason:string;}
export interface ApprovalPort {
  newID():string;
  events(signal:AbortSignal):AsyncIterable<ApprovalEvent>;
  create(request:ApprovalRequest,signal:AbortSignal):Promise<"ask"|"allow"|"deny">;
  reject(id:string,session:string):Promise<void>;
}
/** One event stream. `ready` settles once it reports connected or ends, whichever comes first. */
class Connection {
  readonly controller = new AbortController();
  readonly ready: Promise<void>;
  connected: () => void = () => {};
  ended: Error | undefined;
  constructor() { this.ready = new Promise(resolve => { this.connected = resolve; }); }
}
export class Approval {
  // Waiting starts when the creation response arrives; until then a lost connection does not cancel.
  private readonly pending = new Map<string, {session:string; connection:Connection; waiting:boolean; reply:(error:Error|null)=>void; cancel:(error:Error)=>void}>();
  private connection: Connection;
  private unloaded: Error | undefined;
  constructor(private readonly port:ApprovalPort) {
    this.connection = this.connect();
  }
  private connect() {
    const connection = new Connection();
    void this.listen(connection);
    return connection;
  }
  private async listen(connection:Connection) {
    try {
      for await (const event of this.port.events(connection.controller.signal)) {
        if (connection.ended) break;
        if (event.type === "connected") connection.connected();
        else {
          const pending = this.pending.get(event.id);
          if (pending?.session === event.session) pending.reply(event.reply === "reject" ? new Error("Guardian approval rejected.") : null);
        }
      }
      this.end(connection,new Error("Guardian approval connection ended."));
    } catch {
      this.end(connection,new Error("Guardian approval connection failed."));
    }
  }
  /** The live event stream, reconnected once the previous one has ended. */
  private current() {
    if (this.connection.ended && !this.unloaded) this.connection = this.connect();
    return this.connection;
  }
  private reject(id:string,session:string) {
    void this.port.reject(id,session).catch(() => {});
  }
  /** Resolves "approved" after an approval, or "unavailable" when no approval request could be made. */
  async request(session:string,input:Invocation,reason:string,signal:AbortSignal):Promise<"approved"|"unavailable"> {
    if (signal.aborted) throw new Error("Guardian execution cancelled.");
    if (this.unloaded) throw this.unloaded;
    const connection = this.current();
    let rejectReady: (error:Error)=>void = () => {};
    const cancelledReady=new Promise<never>((_resolve,reject)=>{rejectReady=reject;});
    const abortReady=()=>rejectReady(new Error("Guardian execution cancelled."));
    signal.addEventListener("abort",abortReady,{once:true});
    try{
      if(signal.aborted)abortReady();
      await Promise.race([connection.ready,cancelledReady]);
    }finally{signal.removeEventListener("abort",abortReady);}
    if (this.unloaded) throw this.unloaded;
    if (signal.aborted) throw new Error("Guardian execution cancelled.");
    if (connection.ended) return "unavailable";
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
    const entry = {session,connection,waiting:false,reply:settle,cancel};
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
        // No reject reply: if it was created, rejecting it can also reject the session's other waits.
        return "unavailable";
      }
      const effect = created.effect;
      if (effect !== "ask") throw new Error(`Guardian approval was not requested (${effect}).`);
      entry.waiting = true;
      // The stream that would carry the reply ended during creation, so the reply may be lost.
      if (connection.ended) throw connection.ended;
      const rejected = await outcome;
      if (rejected) throw rejected;
      if (cancellationError) throw cancellationError;
      if (connection.ended) throw connection.ended;
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
  close() {
    this.unloaded ??= new Error("Guardian plugin unloaded.");
    this.end(this.connection,this.unloaded);
    for (const pending of this.pending.values()) pending.cancel(this.unloaded);
  }
  private end(connection:Connection,error:Error) {
    connection.ended ??= error;
    connection.controller.abort(); connection.connected();
    for (const pending of this.pending.values()) if (pending.connection === connection && pending.waiting) pending.cancel(connection.ended);
  }
}
