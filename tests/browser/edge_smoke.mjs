// Optional real-browser smoke test. Uses an isolated profile and never opens the user's profile.
import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { mkdir, mkdtemp, readFile, writeFile } from "node:fs/promises";
import { resolve, join } from "node:path";
import { setTimeout as delay } from "node:timers/promises";

const binary = process.env.RUSTDM_EDGE;
if (!binary) throw new Error("Set RUSTDM_EDGE to the Edge executable path.");
const extension = resolve("dist/browser-extensions/chromium");
const root = resolve("target/browser-smoke");
await mkdir(root, { recursive: true });
const profile = await mkdtemp(join(root, "edge-"));
const child = spawn(binary, ["--headless=new", "--disable-gpu", "--no-first-run",
  "--no-default-browser-check", "--disable-background-networking", "--disable-sync",
  "--remote-debugging-port=0", `--user-data-dir=${profile}`,
  `--disable-extensions-except=${extension}`, `--load-extension=${extension}`, "about:blank"],
  { windowsHide: true, stdio: "ignore" });
child.on("error", (error) => { console.error(error.message); });
let socket;
let sequence = 0;
const pending = new Map();
function send(method, params = {}, sessionId) {
  const id = ++sequence;
  return new Promise((resolve, reject) => {
    const timeout = setTimeout(() => { pending.delete(id); reject(new Error(`CDP timeout: ${method}`)); }, 10000);
    pending.set(id, { resolve: (value) => { clearTimeout(timeout); resolve(value); }, reject });
    socket.send(JSON.stringify({ id, method, params, sessionId }));
  });
}
try {
  let address;
  for (let i = 0; i < 100; i++) {
    try {
      const [port, path] = (await readFile(join(profile, "DevToolsActivePort"), "utf8")).trim().split(/\r?\n/);
      address = `ws://127.0.0.1:${port}${path}`; break;
    } catch { await delay(100); }
  }
  if (!address) throw new Error("Edge did not expose a debugging endpoint in its isolated profile.");
  socket = new WebSocket(address);
  await new Promise((resolve, reject) => { socket.addEventListener("open", resolve, { once: true }); socket.addEventListener("error", reject, { once: true }); });
  socket.addEventListener("message", ({ data }) => {
    const message = JSON.parse(data);
    const waiter = pending.get(message.id);
    if (waiter) { pending.delete(message.id); message.error ? waiter.reject(new Error(message.error.message)) : waiter.resolve(message.result); }
  });
  let worker;
  for (let i = 0; i < 50; i++) {
    const { targetInfos } = await send("Target.getTargets");
    worker = targetInfos.find((target) => target.type === "service_worker" && target.url.startsWith("chrome-extension://ipnpmnioeedgmnhlfameghfgaemfnmjm/"));
    if (worker) break;
    await delay(100);
  }
  assert.ok(worker, "Edge did not load the unpacked extension service worker; this browser may restrict --load-extension.");
  const { sessionId } = await send("Target.attachToTarget", { targetId: worker.targetId, flatten: true });
  const selected = "https://example.com/browser-smoke.bin?selected=1";
  const opened = await send("Runtime.evaluate", { expression: `(async () => {
    // Seed only the browser API draft transport. The real review page loads its
    // own ES modules; dynamic import in a service-worker debugger is disallowed.
    await chrome.contextMenus.update('rustdm-queue-link', {title:'Queue link in RustDM...'});
    const key = 'draft:edge-smoke';
    await chrome.storage.session.set({[key]: {url:${JSON.stringify(selected)}, created:Date.now()}});
    await chrome.tabs.create({url:chrome.runtime.getURL('confirm.html')+'?draft='+encodeURIComponent(key)});
    return chrome.runtime.id;
  })()`, awaitPromise: true, returnByValue: true }, sessionId);
  if (opened.exceptionDetails) throw new Error(opened.exceptionDetails.exception?.description || opened.exceptionDetails.text);
  assert.equal(opened.result.value, "ipnpmnioeedgmnhlfameghfgaemfnmjm");
  let page;
  for (let i = 0; i < 50; i++) {
    const { targetInfos } = await send("Target.getTargets");
    page = targetInfos.find((target) => target.type === "page" && target.url.includes("/confirm.html?draft="));
    if (page) break;
    await delay(100);
  }
  assert.ok(page);
  const { sessionId: pageSession } = await send("Target.attachToTarget", { targetId: page.targetId, flatten: true });
  let form;
  for (let i = 0; i < 50; i++) {
    form = await send("Runtime.evaluate", { expression: `({url:document.querySelector('#url')?.value,filename:document.querySelector('#filename')?.value})`, returnByValue: true }, pageSession);
    if (form.result.value?.url) break;
    await delay(100);
  }
  assert.equal(form.result.value.url, selected);
  assert.equal(form.result.value.filename, "browser-smoke.bin");
  const screenshot = await send("Page.captureScreenshot", {}, pageSession);
  await writeFile(join(profile, "review.png"), Buffer.from(screenshot.data, "base64"));
  if (process.env.RUSTDM_SMOKE_SUBMIT === "1") {
    await send("Runtime.evaluate", { expression: "document.querySelector('#download').requestSubmit()" }, pageSession);
    let status;
    for (let i = 0; i < 100; i++) {
      const result = await send("Runtime.evaluate", { expression: "document.querySelector('#status').textContent", returnByValue: true }, pageSession);
      status = result.result.value;
      if (status?.startsWith("Queued as job")) break;
      await delay(100);
    }
    assert.ok(status?.startsWith("Queued as job"), `Native submission failed: ${status}`);
    console.log("PASS: real Edge review submitted a job through the registered native host.");
  }
  await send("Browser.close");
  console.log(`PASS: Edge loaded extension, selected-link draft opened a real review form. Evidence: ${profile}`);
} finally {
  socket?.close();
  if (child.exitCode === null) child.kill();
}
