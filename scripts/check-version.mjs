import { readFileSync } from 'node:fs'
import { resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

const root = resolve(fileURLToPath(new URL('..', import.meta.url)))
const read = (path) => readFileSync(resolve(root, path), 'utf8')
const json = (path) => JSON.parse(read(path))

const cargoVersion = read('Cargo.toml').match(/^version\s*=\s*"([^"]+)"/m)?.[1]
if (!cargoVersion) throw new Error('Could not read workspace version from Cargo.toml')

const packageLock = json('web-ui/package-lock.json')
const cargoLock = read('Cargo.lock')
const versions = new Map([
  ['Cargo.toml', cargoVersion],
  ['src-tauri/tauri.conf.json', json('src-tauri/tauri.conf.json').version],
  ['web-ui/package.json', json('web-ui/package.json').version],
  ['web-ui/package-lock.json', packageLock.version],
  ['web-ui/package-lock.json packages[""]', packageLock.packages?.['']?.version],
  ['README.md current version', read('README.md').match(/^> 当前版本：`([^`]+)`/m)?.[1]],
  ['README.md document version', read('README.md').match(/^> 文档状态：已按 `v([^`]+)`/m)?.[1]],
  ['MANUAL.md', read('MANUAL.md').match(/^> 适用版本：(\d+\.\d+\.\d+)\s*$/m)?.[1]],
  ['docs/qa/desktop-release.md', read('docs/qa/desktop-release.md').match(/current desktop release, including `v([^`]+)`/)?.[1]],
])

for (const packageName of ['llm-tutor-desktop', 'tutor-agent', 'tutor-rag', 'tutor-tools', 'tutor-web']) {
  const escapedName = packageName.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')
  const pattern = new RegExp(`\\[\\[package\\]\\]\\r?\\nname = "${escapedName}"\\r?\\nversion = "([^"]+)"`)
  versions.set(`Cargo.lock ${packageName}`, cargoLock.match(pattern)?.[1])
}

const mismatches = [...versions].filter(([, version]) => version !== cargoVersion)
if (mismatches.length > 0) {
  const detail = mismatches.map(([source, version]) => `  ${source}: ${version ?? '<missing>'}`).join('\n')
  throw new Error(`Version sources do not match ${cargoVersion}:\n${detail}`)
}

const expectedTag = process.argv[2]
if (expectedTag && expectedTag !== `v${cargoVersion}`) {
  throw new Error(`Release tag ${expectedTag} does not match v${cargoVersion}`)
}

console.log(`Version consistency check passed: ${cargoVersion}`)
