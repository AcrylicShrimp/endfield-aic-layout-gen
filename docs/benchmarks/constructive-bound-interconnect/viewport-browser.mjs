import fs from 'node:fs';
import path from 'node:path';
import { pathToFileURL } from 'node:url';
import { createHash } from 'node:crypto';
import { spawn } from 'node:child_process';
import { setTimeout as delay } from 'node:timers/promises';
import assert from 'node:assert/strict';
const hash = p => createHash('sha256').update(fs.readFileSync(p)).digest('hex');
let chrome, socket, guard, force, closed = false;
try {
  const options = {}, args = process.argv.slice(2), keys = ['browser', 'page', 'focus', 'output-directory'];
  for (let i = 0; i < args.length; i += 2) {
    const k = args[i].slice(2);
    if (!args[i].startsWith('--') || !keys.includes(k) || options[k] || !args[i + 1]) throw new Error(`Invalid argument ${args[i]}`);
    options[k] = args[i + 1];
  }
  for (const k of keys) if (!options[k]) throw new Error(`Missing --${k}`);
  const page = path.resolve(options.page), browser = path.resolve(options.browser), output = path.resolve(options['output-directory']);
  fs.accessSync(page, fs.constants.R_OK); fs.accessSync(browser, fs.constants.X_OK); fs.mkdirSync(output);
  chrome = spawn(browser, ['--no-sandbox', '--disable-gpu', '--remote-debugging-port=0'], { stdio: ['ignore', 'ignore', 'pipe'] });
  const completion = new Promise(resolve => chrome.once('close', () => { closed = true; resolve(); }));
  const stop = () => { if (!closed) { chrome.kill('SIGTERM'); force = setTimeout(() => { if (!closed) chrome.kill('SIGKILL'); }, 1000); } };
  guard = setTimeout(stop, 30000);
  try {
    const endpoint = await new Promise((resolve, reject) => {
      let stderr = ''; chrome.stderr.on('data', chunk => { stderr += chunk; const m = stderr.match(/DevTools listening on (ws:\/\/\S+)/); if (m) resolve(m[1]); });
      chrome.once('error', reject); chrome.once('exit', () => reject(new Error('Browser exited before opening its debug endpoint')));
    });
    socket = new WebSocket(endpoint); await new Promise((resolve, reject) => { socket.addEventListener('open', resolve, { once: true }); socket.addEventListener('error', reject, { once: true }); });
    let sequence = 0; const pending = new Map();
    socket.addEventListener('message', event => { const r = JSON.parse(event.data); if (!r.id) return; const p = pending.get(r.id); pending.delete(r.id); r.error ? p.reject(new Error(JSON.stringify(r.error))) : p.resolve(r.result); });
    socket.addEventListener('close', () => { for (const p of pending.values()) p.reject(new Error('Browser connection closed')); pending.clear(); });
    const call = (method, params = {}, sessionId) => new Promise((resolve, reject) => { const id = ++sequence; pending.set(id, { resolve, reject }); socket.send(JSON.stringify({ id, method, params, ...(sessionId ? { sessionId } : {}) })); });
    const { targetId } = await call('Target.createTarget', { url: 'about:blank' });
    const { sessionId } = await call('Target.attachToTarget', { targetId, flatten: true });
    const c = (method, params) => call(method, params, sessionId);
    const evaluate = async expression => { const r = await c('Runtime.evaluate', { expression, returnByValue: true, awaitPromise: true }); if (r.exceptionDetails) throw new Error(JSON.stringify(r.exceptionDetails)); return r.result.value; };
    const snapshot = () => evaluate('({view:[...view],full:[...fullView],height:svg.clientHeight,width:svg.clientWidth})');
    const capture = async name => { const r = await c('Page.captureScreenshot', { format: 'png' }); fs.writeFileSync(path.join(output, `${name}.png`), Buffer.from(r.data, 'base64'), { flag: 'wx' }); };
    await c('Emulation.setDeviceMetricsOverride', { width: 1400, height: 1400, deviceScaleFactor: 1, mobile: false });
    await c('Page.navigate', { url: pathToFileURL(page).href });
    for (let i = 0; i < 50; i++) { if (await evaluate('typeof fullView!=="undefined"&&!!fullView')) break; await delay(50); }
    assert.equal(await evaluate('validation.covered_geometry_validated'), true);
    const cases = [];
    for (const theme of ['dark', 'light']) for (const width of [1400, 736, 360]) {
      await c('Emulation.setDeviceMetricsOverride', { width, height: 1400, deviceScaleFactor: 1, mobile: false });
      await c('Emulation.setEmulatedMedia', { features: [{ name: 'prefers-color-scheme', value: theme }] });
      await delay(150); await evaluate('document.getElementById("fit").click()');
      const full = await snapshot(); if (width === 1400 && theme === 'dark') await capture('full');
      assert.equal(await evaluate(`focusLabels(${JSON.stringify(options.focus.split(','))})`), true);
      const focused = await snapshot(); assert.notDeepEqual(focused.view, full.view);
      await delay(150); assert.deepEqual(await snapshot(), focused, 'focus must persist beyond resize delivery');
      if (width === 1400 && theme === 'dark') await capture('focus');
      await evaluate('(()=>{const r=svg.getBoundingClientRect();svg.dispatchEvent(new WheelEvent("wheel",{deltaY:-1,clientX:r.x+r.width/2,clientY:r.y+r.height/2,cancelable:true}));})()');
      const zoomed = await snapshot(); assert.ok(zoomed.view[2] < focused.view[2]); await delay(150); assert.deepEqual(await snapshot(), zoomed);
      const center = await evaluate('(()=>{const r=svg.getBoundingClientRect();return{x:r.x+r.width/2,y:r.y+r.height/2};})()');
      await c('Input.dispatchMouseEvent', { type: 'mousePressed', ...center, button: 'left', clickCount: 1 });
      await c('Input.dispatchMouseEvent', { type: 'mouseMoved', x: center.x + 20, y: center.y + 10, button: 'left', buttons: 1 });
      await c('Input.dispatchMouseEvent', { type: 'mouseReleased', x: center.x + 20, y: center.y + 10, button: 'left', clickCount: 1 });
      const panned = await snapshot(); assert.notEqual(panned.view[0], zoomed.view[0]); await delay(150); assert.deepEqual(await snapshot(), panned);
      await evaluate('document.getElementById("fit").click()'); assert.deepEqual((await snapshot()).view, full.full);
      assert.equal(await evaluate('document.documentElement.scrollWidth<=innerWidth'), true);
      cases.push({ width, theme, focus_persists: true, zoom_persists: true, pan_persists: true, fit: true, no_overflow: true });
    }
    const result = { schema_version: 1, all_passed: true, page, page_sha256: hash(page), browser, focus: options.focus.split(','),
      witness_sha256: createHash('sha256').update(await evaluate('JSON.stringify(w)')).digest('hex'), cases };
    fs.writeFileSync(path.join(output, 'report.json'), JSON.stringify(result, null, 2) + '\n', { flag: 'wx' });
    console.log(JSON.stringify({ status: 'viewport-controls-passed', cases: cases.length, output }));
  } finally { socket?.close(); clearTimeout(guard); stop(); await completion; clearTimeout(force); }
} catch (e) { console.error(JSON.stringify({ code: 'viewport-browser-failed', message: e.message })); process.exitCode = 1; }
