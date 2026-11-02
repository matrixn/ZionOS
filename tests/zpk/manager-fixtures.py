"""Build ONLY owned Forge examples in its isolated builder, then seed read-only media."""
from pathlib import Path
import os, shutil, subprocess, tempfile

ROOT = Path(__file__).resolve().parents[2]

def run(*args):
    subprocess.run(args, cwd=ROOT, check=True)

def main():
    seed = ROOT / 'build/manager-fixtures'
    seed.mkdir(parents=True, exist_ok=True)
    target = Path(os.environ['CARGO_TARGET_DIR'])
    release = target / 'x86_64-unknown-linux-musl/release'
    for source, name in [(release/'zpk', 'zpk'), (release/'zpk-manager-guest', 'guest-check'), (release/'zpk-runtime', 'zpk-runtime')]:
        shutil.copyfile(source, seed/name)
        (seed/name).chmod(0o755)
    shutil.copyfile(ROOT/'tests/zpk/manager-guest.sh', seed/'guest.sh')
    forge = ['cargo', 'run', '--locked', '--manifest-path', 'crates/zpk-forge/Cargo.toml', '--bin', 'zpk', '--', 'forge']
    with tempfile.TemporaryDirectory(prefix='manager-projects-', dir=ROOT/'build') as temporary:
        for language, version in [('rust','1.0.0'), ('c','1.0.0'), ('rust','1.1.0'), ('rust','1.2.0')]:
            project = Path(temporary)/f'{language}-{version}'
            project.mkdir()
            example = ROOT/f'examples/zion-hello-{language}'
            inputs = ['zpk.toml','LICENSE','Cargo.toml','Cargo.lock','src'] if language=='rust' else ['zpk.toml','LICENSE','hello.c','CMakeLists.txt']
            for name in inputs:
                source=example/name
                if source.is_dir(): shutil.copytree(source,project/name)
                else: shutil.copyfile(source,project/name)
            manifest=(project/'zpk.toml').read_text().replace('version = "1.0.0"',f'version = "{version}"').replace('upgrades_from = ">=0.1.0, <1.0.0"','upgrades_from = ">=1.0.0, <2.0.0"')
            if version=='1.2.0': manifest=manifest.replace('arguments = ["--serve"]','arguments = ["--fail-health"]')
            (project/'zpk.toml').write_text(manifest)
            output=seed/f'{language}-{version}.zpk'
            for artifact in [output, output.with_suffix('.provenance.json')]:
                if artifact.is_symlink(): raise ValueError('unsafe fixture output')
                if artifact.exists():
                    if not artifact.is_file(): raise ValueError('unsafe fixture output')
                    artifact.unlink()
            run(*forge,'build','--project',str(project))
            run(*forge,'package','--project',str(project),'--output',str(output))

if __name__=='__main__': main()
