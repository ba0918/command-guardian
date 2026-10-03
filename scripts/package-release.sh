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
python3 - "$stage" "$version" "$target" <<'PY'
import json, pathlib, shutil, subprocess, sys
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
metadata=json.loads(subprocess.check_output(['cargo','metadata','--offline','--locked','--filter-platform',sys.argv[3],'--format-version','1']))
packages={package['id']:package for package in metadata['packages']}
nodes={node['id']:node for node in metadata['resolve']['nodes']}
pending=[metadata['resolve']['root']]
selected=set()
while pending:
    identity=pending.pop()
    if identity in selected:
        continue
    selected.add(identity)
    for dependency in nodes[identity]['deps']:
        if any(kind['kind']!='dev' for kind in dependency['dep_kinds']):
            pending.append(dependency['pkg'])
notices=[]
apache_package=next(package for package in packages.values() if package['name']=='ureq' and package['version']=='3.4.2')
apache_text=pathlib.Path(apache_package['manifest_path']).parent/'LICENSE-APACHE'
for package in sorted((packages[identity] for identity in selected),key=lambda p:(p['name'],p['version'])):
    if package['source'] is None:
        continue
    source=pathlib.Path(package['manifest_path']).parent
    destination=stage/'licenses/rust'/f"{package['name']}-{package['version']}"
    texts=[path for path in source.rglob('*') if path.is_file() and
           any(word in path.name.lower() for word in ('license','licence','copying','copyright','notice'))]
    if package.get('license_file'):
        texts.append(source/package['license_file'])
    if not texts:
        if package['license'] not in ('MIT OR Apache-2.0','Apache-2.0 OR MIT'):
            raise SystemExit('Missing dependency license text: '+package['name'])
        destination.mkdir(parents=True,exist_ok=True)
        shutil.copyfile(apache_text,destination/'LICENSE-APACHE')
        notices.append(f"{package['name']} {package['version']}: Apache-2.0 selected from {package['license']}; standard text from ureq 3.4.2; no upstream license or notice file packaged")
        continue
    for text in sorted(set(texts)):
        relative=text.relative_to(source)
        target=destination/relative
        target.parent.mkdir(parents=True,exist_ok=True)
        shutil.copyfile(text,target)
    notices.append(f"{package['name']} {package['version']}: {package['license'] or 'see included license file'}")
(stage/'licenses/rust/NOTICE.txt').write_text(
    'Locked non-dev Rust dependency graph for '+sys.argv[3]+', including build dependencies.\n'+
    '\n'.join(notices)+'\n')
PY
archive="$output/command-guardian-v$version-$target.tar.gz"
tar -czf "$archive" -C "$stage" command-guardian LICENSE opencode licenses
(cd "$output" && sha256sum "$(basename "$archive")" > "$(basename "$archive").sha256")
echo "$archive"
