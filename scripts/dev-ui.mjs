import { spawn } from 'node:child_process';
import { createServer } from 'node:net';
import { fileURLToPath } from 'node:url';
import { setTimeout as delay } from 'node:timers/promises';

const root = fileURLToPath(new URL('../', import.meta.url));
const web = fileURLToPath(new URL('../apps/web/', import.meta.url));
const relayPort = 18080;
const children = new Set();
let stopping = false;

function stop(code = 0) {
  if (stopping) return;
  stopping = true;
  for (const child of children) child.kill('SIGTERM');
  process.exitCode = code;
}

function run(command, args, cwd = root) {
  const child = spawn(command, args, { cwd, stdio: 'inherit' });
  children.add(child);
  child.on('exit', () => children.delete(child));
  child.on('error', error => { console.error(error.message); stop(1); });
  return child;
}

function finished(child) {
  return new Promise((resolve, reject) => {
    child.once('error', reject);
    child.once('exit', code => code === 0 ? resolve() : reject(new Error(`Process exited with ${code}`)));
  });
}

process.on('SIGINT', () => stop());
process.on('SIGTERM', () => stop());

try {
  // Refuse to reuse an unknown relay, even if its health endpoint says sim.
  await new Promise((resolve, reject) => {
    const probe = createServer();
    probe.once('error', reject);
    probe.listen(relayPort, '127.0.0.1', () => probe.close(resolve));
  });
  await finished(run('cargo', ['build', '-p', 'openmix-serve']));
  if (!stopping) {
    const relay = run('./target/debug/openmix-serve', [
      '--sim-only', '--driver', 'sim', '--preset', 'studio',
      '--bind', '127.0.0.1', '--port', String(relayPort),
    ]);
    relay.once('exit', code => stop(code ?? 1));
    let ready = false;
    for (let attempt = 0; attempt < 50 && !stopping; attempt++) {
      try {
        const health = await fetch(`http://127.0.0.1:${relayPort}/healthz`);
        ready = health.ok && (await health.json()).driver === 'sim';
      } catch { /* Relay is still starting. */ }
      if (ready) break;
      await delay(100);
    }
    if (!ready) throw new Error('The development simulator did not start');

    // Seed only this owned, loopback-only fake desk. No hardware is registered
    // through this socket: the relay rejects hardware connects and discovery.
    const socket = new WebSocket(`ws://127.0.0.1:${relayPort}/ws`);
    await new Promise((resolve, reject) => {
      let seeded = false;
      const timeout = setTimeout(() => reject(new Error('Simulator setup timed out')), 5000);
      socket.addEventListener('error', () => {
        if (!seeded) { clearTimeout(timeout); reject(new Error('Simulator setup failed')); }
      }, { once: true });
      socket.addEventListener('message', event => {
        if (typeof event.data !== 'string') return;
        const message = JSON.parse(event.data);
        if (message.type === 'param' && message.ch === 15 && message.field === 'phantom' && message.value === true) {
          seeded = true;
          clearTimeout(timeout);
          resolve();
          socket.close();
        }
      });
      socket.addEventListener('open', () => {
        for (let ch = 0; ch < 16; ch++) {
          socket.send(JSON.stringify({ cmd: 'mute', ch, muted: false }));
          socket.send(JSON.stringify({ cmd: 'fader', ch, value: 0.72 }));
          for (const [field, value] of Object.entries({
            gate_on: ch % 3 === 1, gate_thr: -24, gate_range: 30,
            comp_on: ch % 3 !== 1, comp_thr: -26, comp_ratio: 4,
            phantom: ch % 4 === 3,
          })) socket.send(JSON.stringify({ cmd: 'param', ch, field, value }));
        }
      }, { once: true });
      socket.addEventListener('close', () => {
        if (!seeded) { clearTimeout(timeout); reject(new Error('Simulator closed before setup completed')); }
      }, { once: true });
    });
    console.log('UI development uses a fresh fake desk. Hardware connections and discovery are disabled.');
    const vite = run(process.execPath, ['node_modules/vite/bin/vite.js', '--host', '0.0.0.0', ...process.argv.slice(2)], web);
    vite.once('exit', code => stop(code ?? 1));
  }
} catch (error) {
  console.error(error.message);
  stop(1);
}
