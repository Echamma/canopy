// Runs StreamCuts for Canopy the way StreamCutsLauncher does: the long-to-short backend on :4000 and the editor on
// :3000, `next start` on a production build or, without one, the webpack dev server (Turbopack mis-resolves
// tailwindcss in StreamCuts' bun workspace). If both are already running (say, started from StreamCuts.exe), Canopy
// uses them and leaves them alone; it only ever stops processes it started itself.
const fs = require('node:fs');
const path = require('node:path');
const {spawn, execFile} = require('node:child_process');

const WEB_URL = 'http://localhost:3000';
const HEALTH_URL = 'http://localhost:4000/api/health';
// The same development defaults the launcher uses; anything set in the environment or an .env file wins.
const DEFAULT_ENV = {
  NEXT_PUBLIC_SITE_URL: 'http://localhost:3000',
  NEXT_PUBLIC_MARBLE_API_URL: 'https://example.com',
  DATABASE_URL: 'postgres://user:pass@localhost:5432/opencut',
  BETTER_AUTH_SECRET: 'dev-secret',
  UPSTASH_REDIS_REST_URL: 'https://example.com',
  UPSTASH_REDIS_REST_TOKEN: 'dev-token',
  MARBLE_WORKSPACE_KEY: 'dev-workspace',
  FREESOUND_CLIENT_ID: 'dev-client',
  FREESOUND_API_KEY: 'dev-key',
  NEXT_PUBLIC_LONG_TO_SHORT_API_URL: 'http://localhost:4000',
  PYTHON_BIN: 'python',
  FASTER_WHISPER_MODEL: 'medium'
};
// The Whisper device stays unset, so transcription picks CUDA when it is available.
const BACKEND_ENV = {PORT: '4000', FRONTEND_ORIGIN: 'http://localhost:3000'};

async function isUp(url) {
  try {
    const response = await fetch(url, {signal: AbortSignal.timeout(1500), redirect: 'manual'});
    return response.status < 500;
  } catch { return false; }
}

function envFileKeys(dir, names) {
  const keys = new Set();
  for (const name of names) {
    try {
      for (const line of fs.readFileSync(path.join(dir, name), 'utf8').split(/\r?\n/)) {
        const match = /^\s*(?:export\s+)?([A-Za-z_][A-Za-z0-9_]*)\s*=/.exec(line);
        if (match) keys.add(match[1]);
      }
    } catch {}
  }
  return keys;
}
// Placeholders only fill gaps: process variables override .env files in both Next.js and dotenv, so a key the app's
// own env files (the ones that mode reads) define is left for the app to load.
function environment(dir, envFiles, defaults) {
  const env = {...process.env};
  delete env.ELECTRON_RUN_AS_NODE;
  const fromFiles = envFileKeys(dir, envFiles);
  for (const [key, value] of Object.entries(defaults)) if (env[key] === undefined && !fromFiles.has(key)) env[key] = value;
  return env;
}

function createStreamCutsService({root, logFile}) {
  const backendDir = path.join(root, 'backend', 'long-to-short');
  const webDir = path.join(root, 'opencut-classic', 'apps', 'web');
  const children = [];
  let starting = null;

  // npm is a .cmd script on Windows, which Node only starts through a shell; node itself starts directly.
  function launch(command, args, cwd, env) {
    const log = fs.openSync(logFile, 'a');
    const shell = command === 'npm';
    const child = spawn(command, args, {cwd, env, shell, windowsHide: true, stdio: ['ignore', log, log]});
    fs.closeSync(log);
    children.push(child);
    child.once('exit', () => children.splice(children.indexOf(child), 1));
    return child;
  }
  async function waitFor(url, child, label, timeout) {
    const deadline = Date.now() + timeout;
    while (Date.now() < deadline) {
      if (await isUp(url)) return;
      if (child && child.exitCode !== null) throw new Error(`The StreamCuts ${label} stopped while starting. See ${logFile}.`);
      await new Promise(resolve => setTimeout(resolve, 700));
    }
    throw new Error(`The StreamCuts ${label} didn't start in time. See ${logFile}.`);
  }

  // Resolves with the editor's address once both halves answer.
  function start(onStatus = () => {}) {
    starting ??= (async () => {
      fs.mkdirSync(path.dirname(logFile), {recursive: true});
      const [webUp, backendUp] = await Promise.all([isUp(WEB_URL), isUp(HEALTH_URL)]);
      if (!backendUp) {
        if (!fs.existsSync(path.join(backendDir, 'dist', 'main.js'))) throw new Error(`StreamCuts' backend isn't built yet: run "npm run build" in ${backendDir}.`);
        if (!fs.existsSync(path.join(backendDir, 'node_modules'))) throw new Error(`StreamCuts' backend dependencies are missing: run "npm install" in ${backendDir}.`);
        onStatus('Starting the StreamCuts backend…');
        // dotenv reads only .env from the backend's folder.
        const backend = launch('node', ['dist/main.js'], backendDir, environment(backendDir, ['.env'], {...DEFAULT_ENV, ...BACKEND_ENV}));
        await waitFor(HEALTH_URL, backend, 'backend', 90000);
      }
      if (!webUp) {
        const production = fs.existsSync(path.join(webDir, '.next', 'BUILD_ID'));
        const nextCli = path.join(webDir, 'node_modules', 'next', 'dist', 'bin', 'next');
        if (!production && !fs.existsSync(nextCli)) throw new Error(`StreamCuts' editor has neither a production build nor Next.js installed: run "bun install" in ${path.join(root, 'opencut-classic')}.`);
        onStatus(production ? 'Starting the StreamCuts editor…' : 'Starting the StreamCuts editor. Without a production build it compiles on first open, so give it a minute…');
        const web = production
          ? launch('npm', ['run', 'start', '--', '--port', '3000'], webDir, {...environment(webDir, ['.env', '.env.local', '.env.production', '.env.production.local'], DEFAULT_ENV), NODE_ENV: 'production'})
          : launch('node', [nextCli, 'dev', '--webpack', '--port', '3000'], webDir, environment(webDir, ['.env', '.env.local', '.env.development', '.env.development.local'], DEFAULT_ENV));
        await waitFor(WEB_URL, web, 'editor', production ? 90000 : 240000);
      }
      return WEB_URL + '/projects';
    })().catch(error => { starting = null; throw error; });
    return starting;
  }

  // Stops what Canopy started, with their child processes (npm starts Next.js under it).
  async function stop() {
    starting = null;
    await Promise.all(children.splice(0).map(child => new Promise(resolve => {
      if (child.exitCode !== null || !child.pid) return resolve();
      if (process.platform === 'win32') execFile('taskkill', ['/pid', String(child.pid), '/T', '/F'], () => resolve());
      else { child.kill('SIGTERM'); resolve(); }
    })));
  }

  return {start, stop, url: WEB_URL, owns: () => children.length > 0};
}

module.exports = {createStreamCutsService, isUp, WEB_URL, HEALTH_URL};
