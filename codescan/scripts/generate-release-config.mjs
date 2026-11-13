#!/usr/bin/env node
// 生成 Tauri 条件构建配置：克隆 apps/desktop/src-tauri/tauri.release.conf.json
// → tauri.ci.conf.json（gitignore，构建命令 --config 消费）。
//
//   - updater 可用时：保留 bundle.createUpdaterArtifacts 与 plugins.updater
//     （tauri build 产出 .app.tar.gz + .sig updater 工件，需 TAURI_SIGNING_PRIVATE_KEY）
//   - updater 不可用时：移除 bundle.createUpdaterArtifacts 与 plugins.updater，
//     避免「createUpdaterArtifacts 无插件配置」「空公钥」导致构建/启动失败
//
// 判定必须用「非空」而非「已定义」——CI 里 `${{ secrets.X }}` 对未配置的 secret
// 会注入空字符串（本地链则根本不设），按「已定义」判定会让 updater 带着空密钥
// 激活、构建在最后签名一步失败（axiom 2026-09-22 工作流实跑踩坑，同型规避）。
//
// 用法：node scripts/generate-release-config.mjs [--force-updater] [--force-no-updater]
// 决策优先级：--force 参数 > 环境变量 UPDATER_ENABLED=true/false > 自动检测
// （配置内已提交公钥非空且环境存在非空 TAURI_SIGNING_PRIVATE_KEY）。
// 本地发布链（build:dmg）与 CI 走自动检测；凭据缺失时自动降级（updater 剥离）。

import { readFileSync, writeFileSync } from 'node:fs'
import path from 'node:path'
import { fileURLToPath } from 'node:url'

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..')
const CONFIG_SRC = path.join(root, 'apps/desktop/src-tauri/tauri.release.conf.json')
const CONFIG_OUT = path.join(root, 'apps/desktop/src-tauri/tauri.ci.conf.json')

const forceUpdater = process.argv.includes('--force-updater')
const forceNoUpdater = process.argv.includes('--force-no-updater')
if (forceUpdater && forceNoUpdater) {
  console.error('--force-updater 与 --force-no-updater 不能同时使用')
  process.exit(2)
}

/** tauri.release.conf.json 内已提交的 updater 公钥（公钥可入库，非敏感）。 */
function committedPubkey(config) {
  return config?.plugins?.updater?.pubkey?.trim() ?? ''
}

function resolveUpdaterEnabled(config) {
  if (forceUpdater) return true
  if (forceNoUpdater) return false
  if (process.env.UPDATER_ENABLED !== undefined) {
    return process.env.UPDATER_ENABLED === 'true'
  }
  // 自动检测：公钥已提交 + 本地具备签名私钥（tauri build 打 updater 工件时需要）。
  const updaterKey = process.env.TAURI_SIGNING_PRIVATE_KEY
  return committedPubkey(config) !== '' && typeof updaterKey === 'string' && updaterKey.trim().length > 0
}

const base = JSON.parse(readFileSync(CONFIG_SRC, 'utf8'))
const updaterEnabled = resolveUpdaterEnabled(base)
if (!updaterEnabled) {
  delete base.bundle.createUpdaterArtifacts
  delete base.plugins?.updater
  if (base.plugins && Object.keys(base.plugins).length === 0) delete base.plugins
}
writeFileSync(CONFIG_OUT, JSON.stringify(base, null, 2) + '\n')
console.log(`Generated ${path.relative(root, CONFIG_OUT)} (updater=${updaterEnabled ? 'enabled' : 'disabled'})`)
