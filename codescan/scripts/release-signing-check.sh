#!/usr/bin/env bash
# 发布产物签名验收（发布链 fail-closed 关卡，本地与 CI 共用）。
#
# 用法：release-signing-check.sh [--allow-adhoc] <Codescan.app> [<Codescan_<v>_aarch64.dmg>]
#   --allow-adhoc  放行 adhoc 未签名产物（本地调试 / 无凭据降级验收）；
#                  缺省时 adhoc 判红（正式发布不得带未签名产物出门）。
# 检查项（有签名时）：Developer ID Application 身份、codesign --verify --deep
# --strict、spctl --assess --type execute、stapler validate（.app 与 .dmg）。
set -euo pipefail

ALLOW_ADHOC=0
ARGS=()
for ARG in "$@"; do
  case "$ARG" in
    --allow-adhoc) ALLOW_ADHOC=1 ;;
    --) ;; # pnpm run 透传的分隔符，非路径
    *) ARGS+=("$ARG") ;;
  esac
done
if [ "${#ARGS[@]}" -lt 1 ]; then
  echo '用法: release-signing-check.sh [--allow-adhoc] <.app> [<.dmg>]' >&2
  exit 2
fi
APP="${ARGS[0]}"
DMG="${ARGS[1]:-}"
[ -d "$APP" ] || { echo ".app 不存在: $APP" >&2; exit 2; }

# -dvv 才输出 Authority 证书链（-dv 在新版 macOS 只有 TeamIdentifier）
SIG_INFO="$(codesign -dvv "$APP" 2>&1 || true)"
if echo "$SIG_INFO" | grep -q 'Signature=adhoc'; then
  if [ "$ALLOW_ADHOC" = 1 ]; then
    echo 'adhoc 签名（--allow-adhoc 放行）：跳过签名验收'
    exit 0
  fi
  echo '产物为 adhoc 签名：正式发布需 Developer ID 签名 + 公证（调试请加 --allow-adhoc）' >&2
  exit 1
fi

IDENTITY="$(printf '%s\n' "$SIG_INFO" | sed -n 's/^Authority=//p' | head -1)"
echo "签名身份：$IDENTITY"
printf '%s\n' "$IDENTITY" | grep -q 'Developer ID Application' \
  || { echo '非 Developer ID Application 身份，不得用于发布' >&2; exit 1; }

codesign --verify --deep --strict "$APP"
echo 'codesign verify 通过'
spctl --assess --type execute "$APP"
echo 'spctl assess 通过'

# staple 校验：本地链 stapled .app（bundler 公证 + 补打），CI 链 stapled .dmg
# （notarytool 显式公证）——两链取其一即可；app 未 staple 时 Gatekeeper 走
# 在线公证查询（spctl assess 已覆盖该路径），票据只保证离线校验。
STAPLE_OK=0
if xcrun stapler validate "$APP" >/dev/null 2>&1; then
  STAPLE_OK=1
  echo 'app staple 校验通过'
fi
if [ -n "$DMG" ]; then
  [ -f "$DMG" ] || { echo ".dmg 不存在: $DMG" >&2; exit 2; }
  if xcrun stapler validate "$DMG" >/dev/null 2>&1; then
    STAPLE_OK=1
    echo 'dmg staple 校验通过'
  fi
fi
[ "$STAPLE_OK" = 1 ] || { echo 'app 与 dmg 均无公证票据（staple 缺失）' >&2; exit 1; }
echo '签名验收通过'
