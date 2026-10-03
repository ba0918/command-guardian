import type { ChildProcess } from "node:child_process";

export async function stopHost(server: ChildProcess, graceMs = 1000): Promise<void> {
  if (server.pid === undefined || server.exitCode !== null || server.signalCode !== null) return;
  const wait = (signal: NodeJS.Signals) => new Promise<boolean>(resolve => {
    const finish = (exited: boolean) => {
      clearTimeout(timer);
      server.removeListener("exit", onExit);
      resolve(exited);
    };
    const onExit = () => finish(true);
    const timer = setTimeout(() => finish(false), graceMs);
    server.once("exit", onExit);
    server.kill(signal);
  });
  // A stuck fixture must not hold the test runner indefinitely; this is not a product deadline.
  if (!await wait("SIGTERM") && !await wait("SIGKILL")) throw new Error("Isolated server cleanup did not reap the child");
}
