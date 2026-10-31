// Copies Forma and Folio into apps/ so a packaged Canopy carries them. Each app's own package.json "build.files" list
// says what its packaged app needs; the same files are copied here. Their runtime dependencies (ajv for Forma's MCP
// validation, jszip for Folio) come from Canopy's own node_modules.
//   node scripts/vendor-apps.cjs [--forma <dir>] [--folio <dir>]
const fs = require('node:fs');
const path = require('node:path');
const {execFileSync} = require('node:child_process');

const root = path.join(__dirname, '..');
const argument = name => { const index = process.argv.indexOf('--' + name); return index > 0 ? process.argv[index + 1] : null; };
const sources = {
  forma: argument('forma') || process.env.CANOPY_FORMA_DIR || 'D:\\projects\\forma-studio',
  folio: argument('folio') || process.env.CANOPY_FOLIO_DIR || 'D:\\projects\\folio-carousel-studio'
};

function copy(source, target) {
  const stat = fs.statSync(source);
  if (stat.isDirectory()) {
    fs.mkdirSync(target, {recursive: true});
    for (const name of fs.readdirSync(source)) copy(path.join(source, name), path.join(target, name));
  } else {
    fs.mkdirSync(path.dirname(target), {recursive: true});
    fs.copyFileSync(source, target);
  }
}

for (const [id, dir] of Object.entries(sources)) {
  if (!fs.existsSync(path.join(dir, 'app-host.cjs'))) throw new Error(`${dir} has no app-host.cjs; check out its canopy branch.`);
  // Forma's editor is bundled from src/ first, exactly as its own build does.
  if (fs.existsSync(path.join(dir, 'scripts', 'bundle-renderer.cjs'))) execFileSync(process.execPath, [path.join(dir, 'scripts', 'bundle-renderer.cjs')], {cwd: dir, stdio: 'inherit'});
  const target = path.join(root, 'apps', id);
  fs.rmSync(target, {recursive: true, force: true});
  const patterns = JSON.parse(fs.readFileSync(path.join(dir, 'package.json'), 'utf8')).build.files;
  for (const pattern of patterns) {
    const plain = pattern.replace(/\/\*\*(\/\*)?$/, '');
    if (/[*?]/.test(plain)) throw new Error(`Unsupported pattern in ${id}: ${pattern}`);
    copy(path.join(dir, plain), path.join(target, plain));
  }
  // Keep the app's own package.json beside it, so `require` treats its .cjs files the way the app does.
  copy(path.join(dir, 'package.json'), path.join(target, 'package.json'));
  console.log(`Vendored ${id} from ${dir}`);
}
