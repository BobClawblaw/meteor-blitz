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
//
// MOCK_PAD=1 replaces navigator.getGamepads with a fake standard pad that
// taps A every 2.5 s from 12 s in (after the page has loaded; that walks
// Play > Arcade > World 1 > Stage 1.1 from the main menu) and holds the left
// stick right from 25 s, to check the web build's controller path without a
// real pad. MOCK_PAD=survival instead picks Survival (A, Right, Right, A)
// and then fires continuously, sweeping the right stick around.
//
// Console errors and uncaught exceptions (a wasm panic) are printed as
// "console:" lines.
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
  ws.addEventListener("message", (ev) => {
    const msg = JSON.parse(ev.data);
    if (msg.method === "Runtime.exceptionThrown") {
      console.log("console: exception", msg.params.exceptionDetails?.exception?.description ?? msg.params.exceptionDetails?.text);
    } else if (msg.method === "Runtime.consoleAPICalled" && msg.params.type === "error") {
      console.log("console: error", msg.params.args.map((a) => a.value ?? a.description).join(" "));
    }
  });
  if (process.env.MOCK_PAD) {
    await send("Page.addScriptToEvaluateOnNewDocument", { source: `
      (() => {
        const start = performance.now();
        const button = (v) => ({ pressed: v > 0.5, touched: v > 0, value: v });
        navigator.getGamepads = () => {
          const t = performance.now() - start;
          const buttons = Array.from({ length: 17 }, () => button(0));
          let axes = [0, 0, 0, 0];
          const tap = (at, i) => { if (t > at && t < at + 150) buttons[i] = button(1); };
          if (${JSON.stringify(process.env.MOCK_PAD)} === "survival") {
            // Play, then Right twice to Survival, then A; then fire around.
            tap(12000, 0); tap(14500, 15); tap(15500, 15); tap(16500, 0);
            if (t > 19000) {
              const a = t / 700;
              axes = [0, 0, Math.cos(a), Math.sin(a)];
            }
          } else {
            // A every 2.5 s from 12 s in, four times; then the left stick right.
            for (const at of [12000, 14500, 17000, 19500]) tap(at, 0);
            if (t > 25000) axes = [1, 0, 0, 0];
          }
          return [{ id: "mock", index: 0, connected: true, mapping: "standard", axes, buttons, timestamp: t }];
        };
      })();
    ` });
  }
  await send("Page.navigate", { url });
  await sleep(waitMs);
  for (const [cx, cy] of clicks) {
    for (const type of ["mousePressed", "mouseReleased"]) {
      await send("Input.dispatchMouseEvent",
        { type, x: cx, y: cy, button: "left", clickCount: 1 });
    }
    await sleep(2500);
  }
  // KEYS=Enter,Escape,... presses those keys in order, 2.5 s apart, after
  // the clicks (key names as in KeyboardEvent.key).
  const codes = { Enter: ["Enter", 13], Escape: ["Escape", 27], ArrowUp: ["ArrowUp", 38],
    ArrowDown: ["ArrowDown", 40], ArrowLeft: ["ArrowLeft", 37], ArrowRight: ["ArrowRight", 39] };
  for (const key of (process.env.KEYS ?? "").split(",").filter(Boolean)) {
    const [code, vk] = codes[key] ?? [key, 0];
    for (const type of ["keyDown", "keyUp"]) {
      await send("Input.dispatchKeyEvent", { type, key, code, windowsVirtualKeyCode: vk });
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
