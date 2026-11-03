"""Compile three real versions with Forge; never assemble/patch package archives."""
from pathlib import Path
import hashlib
import json
import shutil
import subprocess
import sys
from images import ROOT, workspace


def main():
    folder = workspace(sys.argv[1])
    seed = folder / 'media'
    seed.mkdir()  # A run is never silently reused.
    forge = ['cargo', 'run', '--locked', '--manifest-path', 'crates/zpk-forge/Cargo.toml',
             '--bin', 'zpk', '--', 'forge']
    evidence = []
    for version in ('1.0.0', '1.1.0', '1.2.0'):
        project = folder / f'project-{version}'
        shutil.copytree(ROOT / 'examples/zion-hello', project,
                        ignore=shutil.ignore_patterns('target', 'build', '.forge', '.zpk-forge'))
        for name in ('Cargo.toml', 'Cargo.lock', 'zpk.toml'):
            path = project / name
            text = path.read_text().replace('version = "1.0.0"', f'version = "{version}"')
            if version == '1.2.0' and name == 'zpk.toml':
                text = text.replace('arguments = ["--serve"]', 'arguments = ["--fail-start"]')
            path.write_text(text)
        output = seed / f'hello-{version}.zpk'
        subprocess.run([*forge, 'build', '--project', str(project)], cwd=ROOT, check=True)
        subprocess.run([*forge, 'package', '--project', str(project), '--output', str(output)], cwd=ROOT, check=True)
        subprocess.run([*forge, 'validate', str(output)], cwd=ROOT, check=True)
        result = subprocess.run([*forge, 'inspect', str(output)], cwd=ROOT, check=True, capture_output=True, text=True)
        inspection = json.loads(result.stdout)
        manifest = inspection['package']['manifest']
        assert manifest['package']['version'] == version
        assert manifest['target'] == {'os': 'linux', 'architecture': 'x86_64', 'cpu': 'x86-64-v1', 'libc': 'musl'}
        provenance = json.loads(output.with_suffix('.provenance.json').read_text())
        digest = hashlib.sha256(output.read_bytes()).hexdigest()
        assert provenance['package_sha256'] == digest and not provenance['authenticated']
        native = provenance['build']['dependencies']
        assert len(native) == 1 and native[0]['needed'] == [] and native[0]['interpreter'] is None
        assert 'LIMITED CPU VERIFICATION' in provenance['build']['cpu_verification']
        assert 'src/settings.rs' in provenance['build']['source_files']
        evidence.append({'version': version, 'sha256': digest, 'validated': True, 'cpu': 'x86-64-v1',
                         'cpu_verification': provenance['build']['cpu_verification']})
    (folder / 'packages.json').write_text(json.dumps(evidence, indent=2) + '\n')
    print('PASS Forge -> Foundation compatibility: three compiled static musl versions, hashes, declared legacy baseline')


if __name__ == '__main__':
    main()
