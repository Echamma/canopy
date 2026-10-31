// The apps Canopy can open. Forma and Folio are Electron apps that export an app-host.cjs; Canopy runs their editor
// inside its own window. StreamCuts is a local web app (Next.js editor and NestJS backend) that Canopy starts and shows.
//
// Where each app lives, first match wins:
//   1. %APPDATA%\Canopy\apps.json, e.g. {"forma": "D:\\projects\\forma-studio", "streamcuts": "C:\\...\\StreamCuts\\p3-integration"}
//   2. CANOPY_FORMA_DIR, CANOPY_FOLIO_DIR, STREAMCUTS_ROOT
//   3. the usual checkouts on this computer, and apps/<id> inside Canopy, where scripts/vendor-apps.cjs puts Forma
//      and Folio for a packaged build. A packaged Canopy prefers its own copies; running from source prefers the
//      checkouts, so changes to Forma or Folio show up without re-vendoring.
const fs = require('node:fs');
const path = require('node:path');

const CATALOG = [
  {id: 'forma', name: 'Forma', description: 'Animate shapes and pictures', accent: 'clay', initial: 'Fo', kind: 'electron',
    env: 'CANOPY_FORMA_DIR', candidates: ['D:\\projects\\forma-studio']},
  {id: 'folio', name: 'Folio', description: 'Make carousels', accent: 'sky', initial: 'Fl', kind: 'electron',
    env: 'CANOPY_FOLIO_DIR', candidates: ['D:\\projects\\folio-carousel-studio']},
  {id: 'streamcuts', name: 'StreamCuts', description: 'Edit videos and cut clips', accent: 'berry', initial: 'SC', kind: 'web',
    env: 'STREAMCUTS_ROOT', candidates: [
      path.join(process.env.USERPROFILE || '', 'OneDrive', 'Documents', 'ChatGPT', 'StreamCuts', 'p3-integration'),
      'D:\\projects\\AppsToIntegrate\\OpenCut'
    ]}
];

const isElectronApp = dir => fs.existsSync(path.join(dir, 'app-host.cjs'));
const isStreamCuts = dir => fs.existsSync(path.join(dir, 'backend', 'long-to-short')) && fs.existsSync(path.join(dir, 'opencut-classic', 'apps', 'web'));

function readOverrides(file) {
  try { return JSON.parse(fs.readFileSync(file, 'utf8')) || {}; } catch { return {}; }
}

// Returns the catalog with each app's folder (or the reason it can't be opened).
function resolveApps({overridesFile, bundledDir, preferBundled}) {
  const overrides = readOverrides(overridesFile);
  return CATALOG.map(entry => {
    const valid = entry.kind === 'web' ? isStreamCuts : isElectronApp;
    const bundled = path.join(bundledDir, entry.id);
    const local = preferBundled ? [bundled, ...entry.candidates] : [...entry.candidates, bundled];
    const candidates = [overrides[entry.id], process.env[entry.env], ...local].filter(Boolean);
    const dir = candidates.find(candidate => valid(candidate)) || null;
    const {candidates: _, env, ...app} = entry;
    return {...app, dir, problem: dir ? null : `${entry.name} wasn't found. Set its folder in ${overridesFile} or ${env}.`};
  });
}

module.exports = {resolveApps, CATALOG};
