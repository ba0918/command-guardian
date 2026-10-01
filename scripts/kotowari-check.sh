#!/usr/bin/env bash
# kotowari check を毎コミットで必須にする。kotowari が無いときは検査できないため失敗する。
set -euo pipefail

if ! command -v kotowari >/dev/null 2>&1; then
  echo "kotowari が見つからないため検査できない" >&2
  exit 1
fi

kotowari check
