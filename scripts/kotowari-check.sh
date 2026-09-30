#!/usr/bin/env bash
# kotowari check の error のうち、テストがまだ無いことを示す requirement_without_test と
# scenario_without_test だけは通す。それ以外の error と、kotowari 自体の停止（終了コード 2）は
# 失敗にする。
set -uo pipefail

out=$(kotowari check --format json)
code=$?
if [ "$code" -eq 2 ]; then
  echo "kotowari check が停止した（終了コード 2）" >&2
  exit 1
fi

blocking=$(printf '%s' "$out" | jq '[.findings[]
  | select(.severity == "error")
  | select(.kind != "requirement_without_test" and .kind != "scenario_without_test")]')

if [ "$(printf '%s' "$blocking" | jq 'length')" -ne 0 ]; then
  printf '%s' "$blocking" | jq -r '.[] | "\(.kind)\t\(.path):\(.line // "-")\t\(.detail // "")"' >&2
  exit 1
fi
