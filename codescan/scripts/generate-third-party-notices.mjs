#!/usr/bin/env node
// 生成实际 macOS 分发依赖的第三方许可声明。
// 包内 license/notice/copying/copyright 文件优先；缺失时仅使用固定 SPDX 模板。
// 不记录时间或本机路径；--check 不写文件，缺失/漂移均失败。

import { execFileSync } from 'node:child_process'
import { existsSync, readFileSync, readdirSync, writeFileSync } from 'node:fs'
import path from 'node:path'
import { fileURLToPath } from 'node:url'

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..')
const templates = path.join(root, 'scripts/license-templates')
const output = path.join(root, 'apps/desktop/src-tauri/THIRD_PARTY_NOTICES.txt')
const check = process.argv.includes('--check')
if (process.argv.slice(2).some((arg) => arg !== '--check')) {
  console.error('Usage: node scripts/generate-third-party-notices.mjs [--check]')
  process.exit(2)
}

function run(command, args) {
  return execFileSync(command, args, {
    cwd: root,
    encoding: 'utf8',
    maxBuffer: 64 * 1024 * 1024,
    stdio: ['ignore', 'pipe', 'inherit'],
  })
}

function licenseFiles(packageDir, explicitLicenseFile) {
  const names = readdirSync(packageDir, { withFileTypes: true })
    .filter((entry) => entry.isFile() && /^(license|licence|notice|copying|copyright)([._-]|$)/i.test(entry.name))
    .map((entry) => path.join(packageDir, entry.name))
  if (explicitLicenseFile) {
    const file = path.resolve(packageDir, explicitLicenseFile)
    if (existsSync(file)) names.push(file)
  }
  return [...new Set(names)].sort()
}

function cargoPackages() {
  const metadata = JSON.parse(run('cargo', [
    'metadata', '--format-version', '1', '--locked',
    '--filter-platform', 'aarch64-apple-darwin',
  ]))
  const workspace = new Set(metadata.workspace_members)
  const nodes = new Map(metadata.resolve.nodes.map((node) => [node.id, node]))
  const reachable = new Set()
  const queue = [...workspace]
  while (queue.length > 0) {
    const id = queue.shift()
    if (reachable.has(id)) continue
    reachable.add(id)
    const node = nodes.get(id)
    if (!node) throw new Error(`Cargo resolve node missing: ${id}`)
    queue.push(...node.dependencies)
  }
  return metadata.packages
    .filter((pkg) => reachable.has(pkg.id) && !workspace.has(pkg.id))
    .map((pkg) => ({
      ecosystem: 'Rust',
      name: pkg.name,
      version: pkg.version,
      license: pkg.license,
      authors: pkg.authors,
      repository: pkg.repository,
      files: licenseFiles(path.dirname(pkg.manifest_path), pkg.license_file),
    }))
}

function metadataNames(value) {
  if (!value) return []
  if (typeof value === 'string') return [value]
  if (Array.isArray(value)) return value.flatMap(metadataNames)
  if (typeof value === 'object') {
    const name = value.name ?? ''
    const email = value.email ? ` <${value.email}>` : ''
    return [`${name}${email}`.trim()].filter(Boolean)
  }
  return []
}

function repositoryUrl(value) {
  if (typeof value === 'string') return value
  return value?.url ?? null
}

function npmPackages() {
  const inventory = JSON.parse(run('pnpm', [
    '--filter', '@codescan/desktop', 'licenses', 'list', '--json', '--prod',
  ]))
  const packages = []
  for (const [groupLicense, entries] of Object.entries(inventory)) {
    for (const entry of entries) {
      for (const packageDir of entry.paths) {
        const manifest = JSON.parse(readFileSync(path.join(packageDir, 'package.json'), 'utf8'))
        packages.push({
          ecosystem: 'JavaScript',
          name: manifest.name ?? entry.name,
          version: manifest.version,
          license: manifest.license ?? groupLicense,
          authors: [...metadataNames(manifest.author), ...metadataNames(manifest.contributors)],
          repository: repositoryUrl(manifest.repository) ?? entry.homepage,
          files: licenseFiles(packageDir),
        })
      }
    }
  }
  return packages
}

function fallbackLicenses(expression) {
  const supported = ['MIT', 'BSD-3-Clause', 'MPL-2.0']
  const clean = (value) => value.trim().replace(/^\(+|\)+$/g, '')
  if (expression.includes(' AND ')) {
    const ids = expression.split(/\s+AND\s+/).map(clean)
    if (ids.every((id) => supported.includes(id))) return ids
  }
  if (expression.includes(' OR ')) {
    const choices = expression.split(/\s+OR\s+/).map(clean)
    if (choices.includes('MIT')) return ['MIT']
  }
  if (supported.includes(clean(expression))) return [clean(expression)]
  throw new Error(`No fixed SPDX fallback for license expression: ${expression}`)
}

function render(pkg) {
  if (!pkg.license) throw new Error(`Missing license expression: ${pkg.ecosystem} ${pkg.name}@${pkg.version}`)
  const authors = [...new Set(pkg.authors.filter(Boolean))]
  const attribution = authors.length > 0 ? authors.join('; ') : `the ${pkg.name} authors`
  if (authors.length === 0 && !pkg.repository) {
    throw new Error(`Missing authors and repository: ${pkg.name}@${pkg.version} (${pkg.license})`)
  }
  const lines = [
    `${pkg.ecosystem}: ${pkg.name}@${pkg.version}`,
    `License: ${pkg.license}`,
    `Authors: ${attribution}`,
  ]
  if (pkg.repository) lines.push(`Repository: ${pkg.repository}`)
  if (pkg.files.length > 0) {
    for (const file of pkg.files) {
      lines.push('', `License text source: package file ${path.basename(file)}`, '', readFileSync(file, 'utf8').trim())
    }
  }
  // SPDX 清单含归属但不含许可证条款，不能替代完整许可文本。
  if (pkg.files.length === 0 || pkg.files.every((file) => file.toLowerCase().endsWith('.spdx'))) {
    const ids = fallbackLicenses(pkg.license)
    const selection = pkg.license.includes(' OR ')
      ? ` Selected from OR expression: ${ids.join(', ')}.`
      : ''
    lines.push('', `License text source: standard SPDX fallback.${selection}`)
    for (const id of ids) {
      const template = readFileSync(path.join(templates, `${id}.txt`), 'utf8')
      lines.push('', `--- ${id} ---`, '', template.replace('{attribution}', attribution).trim())
    }
  }
  return lines.join('\n')
}

const packages = [...cargoPackages(), ...npmPackages()]
  .map((pkg) => ({ ...pkg, sortKey: `${pkg.ecosystem}:${pkg.name}@${pkg.version}` }))
  .sort((a, b) => (a.sortKey < b.sortKey ? -1 : a.sortKey > b.sortKey ? 1 : 0))
const content = [
  'CodeScan Third-Party Notices',
  '============================',
  '',
  'Generated by scripts/generate-third-party-notices.mjs; do not edit manually.',
  'Target: aarch64-apple-darwin. JavaScript inventory: production dependencies.',
  '',
  `Packages: ${packages.length}`,
  '',
  ...packages.flatMap((pkg) => ['', '='.repeat(78), render(pkg)]),
  '',
].join('\n').replace(/\r\n?/g, '\n')

if (check) {
  if (!existsSync(output) || readFileSync(output, 'utf8') !== content) {
    const actual = existsSync(output) ? readFileSync(output, 'utf8') : ''
    const expectedLines = content.split('\n')
    const actualLines = actual.split('\n')
    const limit = Math.max(expectedLines.length, actualLines.length)
    let index = 0
    while (index < limit && expectedLines[index] === actualLines[index]) index++
    console.error('THIRD_PARTY_NOTICES.txt is missing or stale; run pnpm run generate:notices')
    console.error(`First difference at line ${index + 1}; generated lines=${expectedLines.length}, committed lines=${actualLines.length}`)
    console.error(`Generated: ${JSON.stringify(expectedLines[index] ?? '<missing>')}`)
    console.error(`Committed: ${JSON.stringify(actualLines[index] ?? '<missing>')}`)
    process.exit(1)
  }
} else {
  writeFileSync(output, content)
}
console.log(`${check ? 'Verified' : 'Generated'} ${packages.length} third-party package notices`)
