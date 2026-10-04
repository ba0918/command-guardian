import { test, expect, spyOn } from "bun:test";
import plugin from "../../src/index.js";
import type { Context } from "@opencode/plugin/promise/plugin";
import { mkdtemp, mkdir, writeFile, readFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";

type CreateBefore = (input: { command: string; cwd: string; timeout: number; shell: string; env: Record<string, string | undefined> }) => Promise<void> | void;
type Execute = (input: unknown, context: unknown) => Promise<unknown>;

// A host stand-in exposing only the plugin context members the plugin calls.
function host(directory: string, overrides: Record<string, unknown> = {}) {
  const registered: { createBefore?: CreateBefore; execute?: Execute } = {};
  const disposable = { dispose: async () => {} };
  const context = {
    app: { name: "opencode", version: "9.9.9", channel: "latest" },
    location: { directory },
    options: {},
    shell: { hook: async (_name: string, handler: CreateBefore) => { registered.createBefore = handler; return disposable; } },
    permission: { hook: async () => disposable, reply: async () => {} },
    tool: { transform: async (edit: (editor: unknown) => void) => {
      edit({ update: (_id: string, change: (tool: { execute: Execute }) => void) => {
        const tool = { execute: async (input: unknown) => {
          const command = (input as { command: string }).command;
          await registered.createBefore?.({ command, cwd: directory, timeout: 1000, shell: "/bin/bash", env: {} });
          return { content: "executed" };
        } };
        change(tool);
        registered.execute = tool.execute;
      } });
      return disposable;
    } },
    ...overrides,
  };
  return { context: context as unknown as Context, registered };
}

async function fixture(verdict: "allow" | "ask" | "block" = "allow") {
  const root = await mkdtemp(join(tmpdir(), "guardian-setup-"));
  const bin = join(root, "bin"), project = join(root, "project");
  await mkdir(bin); await mkdir(project); await mkdir(join(root, "state"));
  await writeFile(join(bin, "command-guardian"), `#!/bin/sh\ncat >> ${JSON.stringify(join(root, "judged"))}\nprintf '%s' '{"status":"judged","mode":{"enforce":true},"verdict":"${verdict}","reason":"fixture ${verdict}"}'\n`, { mode: 0o700 });
  const previous = { PATH: process.env.PATH, XDG_STATE_HOME: process.env.XDG_STATE_HOME };
  process.env.PATH = `${bin}:${process.env.PATH}`;
  process.env.XDG_STATE_HOME = join(root, "state");
  return {
    root, project,
    restore: async () => {
      for (const [key, value] of Object.entries(previous)) { if (value === undefined) delete process.env[key]; else process.env[key] = value; }
      await rm(root, { recursive: true, force: true });
    },
  };
}

const toolContext = () => ({ sessionID: "session", signal: new AbortController().signal });

// @kotowari[REQ-070, EX-144]
test("ex_144_other_reported_opencode_version_neither_fails_nor_warns", async () => {
  const f = await fixture();
  const warn = spyOn(console, "warn").mockImplementation(() => {});
  try {
    const h = host(f.project);
    const cleanup = await plugin.setup(h.context);
    expect(await h.registered.execute?.({ command: "printf fixture" }, toolContext())).toEqual({ content: "executed" });
    expect(warn).not.toHaveBeenCalled();
    await cleanup?.();
  } finally { warn.mockRestore(); await f.restore(); }
});

// @kotowari[REQ-070, EX-145]
test("ex_145_missing_shell_hook_api_fails_loading_with_its_name", async () => {
  const f = await fixture();
  try {
    const h = host(f.project, { shell: {} });
    const failure = await Promise.resolve(plugin.setup(h.context)).then(() => undefined, (error: unknown) => error);
    expect(failure).toBeInstanceOf(Error);
    expect((failure as Error).message).toContain("shell.hook");
    expect(h.registered.execute).toBeUndefined();
  } finally { await f.restore(); }
});

// @kotowari[REQ-065]
test("req_065_without_connection_judges_the_shell_the_host_starts", async () => {
  const f = await fixture();
  try {
    const h = host(f.project);
    const cleanup = await plugin.setup(h.context);
    expect(await h.registered.execute?.({ command: "printf fixture" }, toolContext())).toEqual({ content: "executed" });
    expect(JSON.parse(await readFile(join(f.root, "judged"), "utf8"))).toEqual({ command: "printf fixture", cwd: f.project, shell: "/bin/bash" });
    await cleanup?.();
  } finally { await f.restore(); }
});

// @kotowari[REQ-065]
test("req_065_without_connection_block_stops_the_start", async () => {
  const f = await fixture("block");
  try {
    const h = host(f.project);
    const cleanup = await plugin.setup(h.context);
    await expect(h.registered.execute?.({ command: "printf fixture" }, toolContext())).rejects.toThrow("fixture block");
    await cleanup?.();
  } finally { await f.restore(); }
});

// @kotowari[REQ-066]
test("req_066_without_connection_ask_is_left_to_opencode", async () => {
  const f = await fixture("ask");
  const warn = spyOn(console, "warn").mockImplementation(() => {});
  try {
    const h = host(f.project);
    const cleanup = await plugin.setup(h.context);
    expect(await h.registered.execute?.({ command: "printf fixture" }, toolContext())).toEqual({ content: "executed" });
    expect(warn).not.toHaveBeenCalled();
    await cleanup?.();
  } finally { warn.mockRestore(); await f.restore(); }
});

// @kotowari[REQ-065, REQ-052]
test("req_065_failed_explicit_connection_warns_at_load_and_on_each_execution", async () => {
  const f = await fixture();
  const warn = spyOn(console, "warn").mockImplementation(() => {});
  // The explicit server rejects the credentials; nothing leaves the process.
  const transport = spyOn(globalThis, "fetch").mockImplementation(Object.assign(async () => new Response("unauthorized", { status: 401 }), fetch));
  process.env.GUARDIAN_SETUP_TEST_PASSWORD = "fixture-password-not-real";
  try {
    const h = host(f.project, { options: { serverUrl: "http://127.0.0.1:4097", passwordEnv: "GUARDIAN_SETUP_TEST_PASSWORD" } });
    const cleanup = await plugin.setup(h.context);
    expect(warn).toHaveBeenCalledTimes(1);
    for (let i = 0; i < 2; i++) expect(await h.registered.execute?.({ command: "printf fixture" }, toolContext())).toEqual({ content: "executed" });
    expect(warn).toHaveBeenCalledTimes(3);
    expect(String(warn.mock.calls[0]?.[0])).toContain("serverUrl");
    await cleanup?.();
  } finally { delete process.env.GUARDIAN_SETUP_TEST_PASSWORD; transport.mockRestore(); warn.mockRestore(); await f.restore(); }
});
