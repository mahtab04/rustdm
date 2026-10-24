import test from "node:test";
import assert from "node:assert/strict";
import { readFile, mkdtemp, mkdir, writeFile, rm, realpath } from "node:fs/promises";
import { tmpdir, endianness } from "node:os";
import { join, dirname, basename } from "node:path";
import { spawn } from "node:child_process";
import { createServer } from "node:http";
import { enqueue, openConfirmation, takeDraft } from "../../extensions/browser/extension/bridge.js";
import { fileUrl, suggestFilename, validateFilename, request, response } from "../../extensions/browser/extension/protocol.js";

const cases = JSON.parse(await readFile(new URL("filename_cases.json", import.meta.url)));
test("extension filenames match the native host corpus", () => {
  for (const item of cases) {
    if (item.valid) assert.equal(validateFilename(item.name), item.name);
    else assert.throws(() => validateFilename(item.name), undefined, item.name);
  }
  assert.throws(() => validateFilename("é".repeat(91)));
});
test("only public-style HTTP/HTTPS URLs without embedded credentials are accepted", () => {
  for (const value of ["file:///secret", "javascript:alert(1)", "https://user:pass@example.test/file", "not a URL"]) {
    assert.throws(() => fileUrl(value));
  }
  assert.equal(fileUrl("https://example.test/file#part"), "https://example.test/file");
});
test("filename suggestions strip URL queries and handle encoded separators and reserved names", () => {
  assert.equal(suggestFilename("https://example.test/report.zip?token=abc"), "report.zip");
  assert.equal(suggestFilename("https://example.test/a%2Fb.txt"), "a_b.txt");
  assert.equal(suggestFilename("https://example.test/CON"), "download.bin");
  assert.equal(suggestFilename("https://example.test/%zz"), "download.bin");
});
test("responses are correlated and native errors are visible", () => {
  const message = request("https://example.test/file", "file.bin", "request-1");
  assert.deepEqual(Object.keys(message), ["version", "id", "action", "url", "filename"]);
  assert.throws(() => response({ version: 1, id: "wrong", ok: true }, message.id));
  assert.throws(() => response({ version: 1, id: message.id, ok: false, error: "Duplicate filename" }, message.id), /Duplicate/);
  assert.throws(() => response({ version: 1, id: message.id, ok: true, job_id: -1 }, message.id));
});

function browser(transport) {
  const session = new Map();
  const tabs = [];
  return {
    session, tabsCreated: tabs,
    runtime: { getURL: (path) => `chrome-extension://ipnpmnioeedgmnhlfameghfgaemfnmjm/${path}`, sendNativeMessage: transport },
    storage: { session: {
      set: async (values) => { for (const [key, value] of Object.entries(values)) session.set(key, value); },
      get: async (key) => ({ [key]: session.get(key) }),
      remove: async (key) => { session.delete(key); },
    } },
    tabs: { create: async (tab) => { tabs.push(tab); return { id: 1 }; } },
  };
}
test("selected URLs use single-use session drafts instead of being embedded in tab addresses", async () => {
  const api = browser();
  const url = "https://example.test/file?token=private-value";
  await openConfirmation(api, url);
  assert.equal(api.tabsCreated.length, 1);
  assert.ok(!api.tabsCreated[0].url.includes("private-value"));
  const key = new URL(api.tabsCreated[0].url).searchParams.get("draft");
  assert.equal(await takeDraft(api, key), url);
  assert.equal(await takeDraft(api, key), "");
  api.session.set("draft:old", { url, created: Date.now() - 11 * 60 * 1000 });
  assert.equal(await takeDraft(api, "draft:old"), "");
});
test("no native request is sent before review, and invalid forms never reach the host", async () => {
  let calls = 0;
  const api = browser(async () => { calls++; throw new Error("Native host unavailable"); });
  await openConfirmation(api, "https://example.test/file");
  assert.equal(calls, 0);
  await assert.rejects(enqueue(api, "https://example.test/file", "../bad"));
  assert.equal(calls, 0);
  await assert.rejects(enqueue(api, "https://example.test/file", "file.bin"), /Native host unavailable/);
  assert.equal(calls, 1);
});
test("a failed confirmation tab removes its temporary draft", async () => {
  const api = browser();
  api.tabs.create = async () => { throw new Error("Tab unavailable"); };
  await assert.rejects(openConfirmation(api, "https://example.test/file"));
  assert.equal(api.session.size, 0);
});

test("background registers the link menu and opens review without sending a native request", async () => {
  let nativeCalls = 0;
  const api = browser(async () => { nativeCalls++; });
  const installed = [], clicked = [], menus = [];
  api.runtime.onInstalled = { addListener: (listener) => installed.push(listener) };
  api.contextMenus = {
    removeAll: async () => {},
    create: (menu) => { menus.push(menu); },
    onClicked: { addListener: (listener) => clicked.push(listener) },
  };
  const previous = globalThis.chrome;
  globalThis.chrome = api;
  try {
    await import("../../extensions/browser/extension/background.js");
    installed[0]();
    await new Promise((resolve) => setImmediate(resolve));
    assert.deepEqual(menus[0].contexts, ["link"]);
    clicked[0]({ menuItemId: menus[0].id, linkUrl: "https://example.test/file.zip" });
    await new Promise((resolve) => setImmediate(resolve));
    assert.equal(api.tabsCreated.length, 1);
    assert.equal(nativeCalls, 0);
  } finally { globalThis.chrome = previous; }
});

function command(binary, args, input) {
  return new Promise((resolve, reject) => {
    const child = spawn(binary, args, { windowsHide: true, timeout: 15000 });
    const stdout = [], stderr = [];
    child.stdout.on("data", (data) => stdout.push(data));
    child.stderr.on("data", (data) => stderr.push(data));
    child.on("error", reject);
    child.on("close", (code) => code === 0 ? resolve(Buffer.concat(stdout)) : reject(new Error(Buffer.concat(stderr).toString())));
    child.stdin.on("error", reject);
    child.stdin.end(input);
  });
}

test("extension bridge -> framed native process -> persistent queue -> CLI HTTP download", {
  skip: !process.env.RUSTDM_NATIVE_HOST || !process.env.RUSTDM_CLI,
}, async () => {
  const root = await realpath(tmpdir());
  const temporary = await mkdtemp(join(root, "rustdm-browser-"));
  const downloads = join(temporary, "downloads");
  await mkdir(downloads);
  const state = join(temporary, "queue");
  const caller = "chrome-extension://ipnpmnioeedgmnhlfameghfgaemfnmjm/";
  const config = join(temporary, "browser-host.json");
  await writeFile(config, JSON.stringify({ version: 1, state_dir: state, download_directory: downloads, allowed_callers: [caller] }));
  const data = Buffer.from("RustDM browser integration: exact file bytes\n".repeat(100));
  const server = createServer((_, reply) => { reply.writeHead(200, { "Content-Length": data.length }); reply.end(data); });
  await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
  try {
    const api = browser(async (name, message) => {
      assert.equal(name, "com.rustdm.bridge");
      const payload = Buffer.from(JSON.stringify(message));
      const header = Buffer.alloc(4);
      endianness() === "LE" ? header.writeUInt32LE(payload.length) : header.writeUInt32BE(payload.length);
      const output = await command(process.env.RUSTDM_NATIVE_HOST, ["--config", config, caller], Buffer.concat([header, payload]));
      const length = endianness() === "LE" ? output.readUInt32LE() : output.readUInt32BE();
      assert.equal(output.length, length + 4, "stdout must contain exactly one framed response");
      return JSON.parse(output.subarray(4));
    });
    const url = `http://127.0.0.1:${server.address().port}/file.bin`;
    await openConfirmation(api, url);
    const key = new URL(api.tabsCreated[0].url).searchParams.get("draft");
    const draft = await takeDraft(api, key);
    const queued = await enqueue(api, draft, "browser.bin");
    assert.equal(queued.job_id, 1);
    const before = JSON.parse(await readFile(join(state, "queue.json")));
    assert.equal(before.jobs[0].state, "pending");
    await command(process.env.RUSTDM_CLI, ["--state-dir", state, "queue", "run"]);
    assert.deepEqual(await readFile(join(downloads, "browser.bin")), data);
    const after = JSON.parse(await readFile(join(state, "queue.json")));
    assert.equal(after.jobs[0].state, "completed");
  } finally {
    server.closeAllConnections();
    await new Promise((resolve) => server.close(resolve));
    assert.equal(await realpath(temporary), temporary);
    assert.equal(dirname(temporary), root);
    assert.ok(basename(temporary).startsWith("rustdm-browser-"));
    await rm(temporary, { recursive: true, force: true });
  }
});
