"""Build read-only seed media from owned fixtures; never change tracked example source."""
from pathlib import Path
import hashlib,json,os,shutil
ROOT=Path(__file__).resolve().parents[2]

def main():
    seed=ROOT/'build/runtime-fixtures';seed.mkdir(parents=True,exist_ok=True)
    target=Path(os.environ.get('CARGO_TARGET_DIR',ROOT/'crates/zpk-runtime/target'))/'x86_64-unknown-linux-musl/release'
    for source,name in [('zpk-runtime','zpk-runtime'),('zpk-runtime-demo','runtime-demo'),('zpk-runtime-guest','guest-check')]:
        shutil.copyfile(target/source,seed/name);(seed/name).chmod(0o755)
    example=ROOT/'examples/zpk-runtime-demo'
    for name in ['manifest.toml','runtime.toml','LICENSE']:shutil.copyfile(example/name,seed/name)
    files={'manifest.toml':(seed/'manifest.toml').read_bytes(),'payload/bin/runtime-demo':(seed/'runtime-demo').read_bytes(),'LICENSE':(seed/'LICENSE').read_bytes()}
    (seed/'checksums.json').write_text(json.dumps({'schema_version':1,'algorithm':'sha256','files':{p:hashlib.sha256(data).hexdigest()for p,data in files.items()}}))
    shutil.copyfile(ROOT/'tests/zpk/runtime-guest.sh',seed/'guest.sh')

if __name__=='__main__':main()
