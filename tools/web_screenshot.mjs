#!/usr/bin/env node
// Screenshot the web build in headless Chrome and report what the canvas
// holds, for verifying that the wasm bundle actually renders.
//
//   node tools/web_screenshot.mjs URL OUT.png [WAIT_MS] [WIDTH HEIGHT] [X,Y ...]
//
// The trailing X,Y pairs are clicked in order (2.5 s apart) before the shot,
// e.g. 240,240 134,182 95,112 93,115 enters stage 1.1.
//
// Starts its own headless Chrome with remote debugging, waits WAIT_MS of real
// time (virtual time freezes the game's first frames), captures through the
// DevTools Protocol, and exits. Set CHROME to override the browser path.
import { spawn } from "node:child_process";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

const url = process.argv[2];
const out = process.argv[3];
if (!url || !out) {
  console.error("usage: web_screenshot.mjs URL OUT.png [WAIT_MS] [WIDTH HEIGHT] [X,Y ...]");
  process.exit(2);
}
const waitMs = Number(process.argv[4] ?? 12000);
const width = Number(process.argv[5] ?? 0);
const height = Number(process.argv[6] ?? 0);
const clicks = [];
for (let i = 7; i + 1 < process.argv.length; i += 2) {
  clicks.push([Number(process.argv[i]), Number(process.argv[i + 1])]);
}
const port = 9223;
const chrome = process.env.CHROME ??
  "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome";
const profile = mkdtempSync(join(tmpdir(), "mb-chrome-"));

const child = spawn(chrome, [
  "--headless=new",
  "--no-sandbox",
  `--remote-debugging-port=${port}`,
  `--user-data-dir=${profile}`,
  "--use-angle=swiftshader",
  "--enable-unsafe-swiftshader",
  "--window-size=1280,720",
  "about:blank",
], { stdio: "ignore" });

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

async function targets() {
  for (let i = 0; i < 60; i++) {
    try {
      const list = await (await fetch(`http://127.0.0.1:${port}/json/list`)).json();
      const page = list.find((t) => t.type === "page");
      if (page) return page;
    } catch {}
    await sleep(250);
  }
  throw new Error("Chrome did not expose a page target");
}

try {
  const page = await targets();
  const ws = new WebSocket(page.webSocketDebuggerUrl);
  await new Promise((res, rej) => { ws.onopen = res; ws.onerror = rej; });

  let id = 0;
  const pending = new Map();
  ws.onmessage = (ev) => {
    const msg = JSON.parse(ev.data);
    if (msg.id && pending.has(msg.id)) {
      const { resolve, reject } = pending.get(msg.id);
      pending.delete(msg.id);
      msg.error ? reject(new Error(JSON.stringify(msg.error))) : resolve(msg.result);
    }
  };
  const send = (method, params = {}) => new Promise((resolve, reject) => {
    const mid = ++id;
    pending.set(mid, { resolve, reject });
    ws.send(JSON.stringify({ id: mid, method, params }));
  });

  if (width && height) {
    await send("Emulation.setDeviceMetricsOverride",
      { width, height, deviceScaleFactor: 1, mobile: false });
  }
  await send("Page.enable");
  await send("Runtime.enable");
  await send("Page.navigate", { url });
  await sleep(waitMs);
  for (const [cx, cy] of clicks) {
    for (const type of ["mousePressed", "mouseReleased"]) {
      await send("Input.dispatchMouseEvent",
        { type, x: cx, y: cy, button: "left", clickCount: 1 });
    }
    await sleep(2500);
  }

  const r = await send("Runtime.evaluate", {
    expression: `(() => {
      const c = document.getElementById("glcanvas");
      return { title: document.title, canvas: c ? [c.width, c.height] : null };
    })()`,
    returnByValue: true,
  });
  console.log("page:", JSON.stringify(r.result?.value));

  const shot = await send("Page.captureScreenshot", { format: "png", fromSurface: true });
  writeFileSync(out, Buffer.from(shot.data, "base64"));
  console.log("wrote", out);
  ws.close();
} finally {
  child.kill();
  await new Promise((r) => child.once("exit", r));
  try {
    rmSync(profile, { recursive: true, force: true, maxRetries: 10, retryDelay: 200 });
  } catch (e) {
    console.warn("left profile dir behind:", profile);
  }
}
