#!/usr/bin/env bash
set -euo pipefail

if [[ $# != 3 ]]; then
  echo "Usage: scripts/package-release.sh BINARY TARGET OUTPUT_DIRECTORY" >&2
  exit 2
fi
binary=$(realpath "$1")
target=$2
output=$(realpath -m "$3")
root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$root"
test -x "$binary"
[[ $target == x86_64-unknown-linux-gnu || $target == x86_64-unknown-linux-musl ]]
mkdir -p "$output"
stage=$(mktemp -d "$output/.guardian-package.XXXXXX")
trap 'rm -rf "$stage"' EXIT
version=$(python3 -c 'import tomllib; print(tomllib.load(open("Cargo.toml", "rb"))["package"]["version"])')
mkdir -p "$stage/opencode/source" "$stage/opencode/licenses"
cp "$binary" "$stage/command-guardian"
cp LICENSE "$stage/LICENSE"
cp plugins/opencode/src/*.ts "$stage/opencode/source/"
cp plugins/opencode/licenses/opencode-LICENSE.txt "$stage/opencode/licenses/"
bun build plugins/opencode/index.ts --target=node --format=esm --reject-unresolved \
  --env=disable --outfile="$stage/opencode/server.js" --metafile="$stage/build-meta.json"
python3 - "$stage" "$version" <<'PY'
import json, pathlib, shutil, sys
stage=pathlib.Path(sys.argv[1])
manifest={'name':'command-guardian-opencode','version':sys.argv[2],'type':'module','license':'MIT'}
(stage/'opencode/package.json').write_text(json.dumps(manifest,indent=2)+'\n')
owners=set()
meta=json.loads((stage/'build-meta.json').read_text())
for name in meta['inputs']:
    source=pathlib.Path(name).resolve()
    if 'node_modules' not in source.parts:
        continue
    for parent in [source.parent,*source.parents]:
        package=parent/'package.json'
        if not package.is_file():
            continue
        data=json.loads(package.read_text()); owner=data['name']
        if owner not in {'effect','@opencode/plugin','@opencode/client','@opencode/schema'}:
            raise SystemExit('Undeclared bundled dependency: '+owner)
        if data.get('license')!='MIT':
            raise SystemExit('Bundled license requires review: '+owner)
        owners.add(owner)
        if owner=='effect':
            shutil.copyfile(parent/'LICENSE',stage/'opencode/licenses/effect-LICENSE.txt')
        elif data['version']!='2.0.21':
            raise SystemExit('Unexpected OpenCode runtime version: '+owner)
        break
(stage/'opencode/licenses/NOTICE.txt').write_text(
    'Bundled runtime dependencies: '+', '.join(sorted(owners))+'\n'
    'OpenCode license source: https://github.com/anomalyco/opencode/blob/v2.0.21/LICENSE\n'
    'Effect license source: the installed effect@4.0.0-rc.112 LICENSE.\n')
PY
archive="$output/command-guardian-v$version-$target.tar.gz"
tar -czf "$archive" -C "$stage" command-guardian LICENSE opencode
(cd "$output" && sha256sum "$(basename "$archive")" > "$(basename "$archive").sha256")
echo "$archive"
