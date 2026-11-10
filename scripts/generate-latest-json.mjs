#!/usr/bin/env node
// 生成 Tauri updater 标准静态清单 latest.json（v2 格式），发布链在 Release
// 资产就绪后调用。端点为 GitHub Releases 静态直链
// （https://github.com/amuluze/codescan/releases/latest/download/latest.json，
// draft release 发布后生效），同时作为 Release 资产留档；下载 URL 指向同
// release 的更新工件，签名取自 tauri build 产出的 .sig 文件全文。
//
// 用法：
//   node scripts/generate-latest-json.mjs --tag v0.1.0 \
//     --sig <Codescan.app.tar.gz.sig 路径> \
//     --tarball-url <更新工件下载直链> \
//     --out <latest.json 输出路径> \
//     [--notes <更新说明>]
//
// 版本号取 apps/desktop/src-tauri/tauri.conf.json（与 DMG/更新工件命名同源，
// --tag 仅要求 v 前缀格式，与配置版本的一致性由发布流程前置保证）。
// 平台键 darwin-aarch64 与 updater 客户端 darwin_${arch} 一致。

import { readFileSync, writeFileSync } from 'node:fs'
import path from 'node:path'
import { fileURLToPath } from 'node:url'

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..')

function parseArgs(argv) {
  const args = {}
  for (let index = 0; index < argv.length; index += 2) {
    const key = argv[index]
    if (!key.startsWith('--')) {
      throw new Error(`无法识别的参数：${key}（应为 --key value 形式）`)
    }
    args[key.slice(2)] = argv[index + 1] ?? ''
  }
  return args
}

const args = parseArgs(process.argv.slice(2))
const { tag, sig, 'tarball-url': tarballUrl, out, notes } = args
for (const [name, value] of [['--tag', tag], ['--sig', sig], ['--tarball-url', tarballUrl], ['--out', out]]) {
  if (!value) {
    console.error(`缺少必填参数 ${name}`)
    process.exit(2)
  }
}
if (!/^v\d/.test(tag)) {
  console.error(`--tag 须为 v 前缀格式，收到：${tag}`)
  process.exit(2)
}

const version = JSON.parse(
  readFileSync(path.join(root, 'apps/desktop/src-tauri/tauri.conf.json'), 'utf8'),
).version

const signature = readFileSync(sig, 'utf8').trim()
if (signature === '') {
  console.error(`签名文件为空：${sig}`)
  process.exit(2)
}

const manifest = {
  version,
  notes: notes ?? '',
  pub_date: new Date().toISOString(),
  platforms: {
    'darwin-aarch64': { signature, url: tarballUrl },
  },
}

writeFileSync(out, JSON.stringify(manifest, null, 2) + '\n')
console.log(`Generated ${path.relative(root, out) || out} (darwin-aarch64 @ ${version})`)
