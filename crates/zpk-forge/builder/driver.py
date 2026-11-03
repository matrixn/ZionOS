#!/usr/bin/env python3
"""Trusted driver. Runs ONLY in the disposable, unprivileged, networkless container."""
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys


def compiler(name):
    if shutil.which(name) is None:
        raise RuntimeError(f"missing compiler/tool in isolated builder: {name}")


def run(arguments, environment):
    subprocess.run(arguments, cwd='/work/source', env=environment, check=True)


def main():
    status=dict(line.split(':',1) for line in Path('/proc/self/status').read_text().splitlines() if ':' in line)
    if os.getuid()!=10000 or int(status['CapEff'].strip(),16)!=0 or status['NoNewPrivs'].strip()!='1' or status['Seccomp'].strip()!='2':
        raise RuntimeError('required non-root/capability/no-new-privileges/seccomp isolation is unavailable')
    if set(p.name for p in Path('/sys/class/net').iterdir())!={'lo'}:
        raise RuntimeError('source build network is not isolated')
    cgroup=Path('/sys/fs/cgroup')
    if (cgroup/'memory.max').read_text().strip()!='1073741824' or (cgroup/'pids.max').read_text().strip()!='128':
        raise RuntimeError('required cgroup v2 memory/PID limits are unavailable')
    quota,period=(cgroup/'cpu.max').read_text().split()
    if quota=='max' or int(quota)>2*int(period):
        raise RuntimeError('required CPU quota is unavailable')
    root_mount=next(line.split()[3] for line in Path('/proc/mounts').read_text().splitlines() if line.split()[1]=='/')
    if 'ro' not in root_mount.split(',') or Path('/var/run/docker.sock').exists():
        raise RuntimeError('root filesystem must be read-only and Docker socket absent')
    control=json.loads(Path('/work/input/control.json').read_text())
    recipe=control['recipe']
    shutil.copytree('/work/input/source', '/work/source')
    Path('/work/build').mkdir()
    Path('/work/dest').mkdir()
    Path('/work/export').mkdir()
    environment={
        'PATH':'/usr/sbin:/usr/bin:/sbin:/bin', 'HOME':'/work/home', 'TMPDIR':'/tmp',
        'LANG':'C', 'LC_ALL':'C', 'SOURCE_DATE_EPOCH':'0',
        'CARGO_HOME':'/work/cargo-home', 'CARGO_TARGET_DIR':'/work/build/cargo',
        'RUSTFLAGS':'-C target-cpu=x86-64 --remap-path-prefix=/work/source=/src',
        'CC':'gcc', 'CXX':'g++', 'CFLAGS':'-march=x86-64 -mtune=generic -O2',
        'CXXFLAGS':'-march=x86-64 -mtune=generic -O2', 'LDFLAGS':'',
        'GOOS':'linux', 'GOARCH':'amd64', 'GOAMD64':'v1', 'CGO_ENABLED':'0',
        'GOTOOLCHAIN':'local', 'GOPROXY':'off', 'GOSUMDB':'off', 'GOPATH':'/work/go',
        'GOCACHE':'/work/go-cache', 'DESTDIR':'/work/dest',
    }
    Path('/work/home').mkdir()
    system=recipe['system']; jobs=str(recipe['jobs'])
    versions={}
    tools={'rust':['rustc','cargo'], 'cmake':['gcc','g++','cmake','make'],
           'go':['go'], 'make':['gcc','g++','make']}[system]
    for tool in tools:
        compiler(tool)
        args=[tool,'version'] if tool=='go' else [tool,'--version']
        versions[tool]=subprocess.check_output(args,env=environment,text=True).splitlines()[0]
    if system=='rust':
        target=next(line.split(': ',1)[1] for line in subprocess.check_output(['rustc','-vV'],text=True).splitlines() if line.startswith('host: '))
        if not target.startswith('x86_64-') or not target.endswith('linux-musl'):
            raise RuntimeError('builder Rust target is not x86_64 musl')
        native=next(f for f in control['manifest']['files'] if f['kind']=='executable')
        environment['RUSTFLAGS']+=' -C target-feature='+('+' if native['linkage']=='static' else '-')+'crt-static'
        run(['cargo','build','--offline','--locked','--release','--target',target,'--jobs',jobs,'--bin',recipe['cargo_bin']],environment)
        shutil.copyfile(Path('/work/build/cargo')/target/'release'/recipe['cargo_bin'],'/work/build/forge-app')
    elif system=='cmake':
        run(['cmake','-S','/work/source','-B','/work/build/cmake','-DCMAKE_BUILD_TYPE=Release',
             '-DCMAKE_INSTALL_PREFIX=/usr','-DCMAKE_C_FLAGS=-march=x86-64 -mtune=generic',
             '-DCMAKE_CXX_FLAGS=-march=x86-64 -mtune=generic'],environment)
        run(['cmake','--build','/work/build/cmake','--parallel',jobs],environment)
        run(['cmake','--install','/work/build/cmake'],environment)
    elif system=='go':
        mode='vendor' if Path('/work/source/vendor').is_dir() else 'readonly'
        run(['go','build','-p',jobs,'-mod='+mode,'-trimpath','-buildvcs=false','-ldflags=-buildid=',
             '-o','/work/build/forge-app',recipe['go_package']],environment)
    elif system=='make':
        run(['make','-j'+jobs,recipe['make_target']],environment)
    else:
        raise RuntimeError('unsupported build system')
    for rule in recipe['stage']:
        source=Path('/work')/rule['from']
        # The host validates these inputs again; never use the output tar as an extraction instruction.
        if not source.is_file() or source.is_symlink():
            raise RuntimeError('staging source missing/not a regular file: '+rule['from'])
        destination=Path('/work/export')/rule['to']
        destination.parent.mkdir(parents=True,exist_ok=True)
        shutil.copyfile(source,destination)
        if rule['to'].startswith('payload/bin/'):
            headers=subprocess.check_output(['readelf','-l',str(destination)],text=True)
            if 'INTERP' in headers:
                subprocess.run(['patchelf','--set-rpath','$ORIGIN/../lib',str(destination)],check=True)
    shutil.copyfile(Path('/work/source')/recipe['license_file'],'/work/export/LICENSE')
    Path('/work/toolchains.json').write_text(json.dumps({'versions':versions,'environment':environment,'isolation_checks':'uid/caps/no-new-privileges/seccomp/network/cgroup-v2/root-readonly passed before source execution'},sort_keys=True))


if __name__=='__main__':
    try:
        if len(sys.argv)==3 and sys.argv[1]=='--library':
            name=sys.argv[2]
            if not name.startswith('lib') or '/' in name or any(c not in 'abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789._+-' for c in name):
                raise RuntimeError('unsafe library name')
            source=next((p for p in [Path('/usr/lib')/name,Path('/lib')/name] if p.is_file()),None)
            if source is None: raise RuntimeError('library unavailable in trusted builder: '+name)
            directory=Path('/work/forge-libs'); directory.mkdir(exist_ok=True)
            target=directory/name; shutil.copyfile(source,target)
            subprocess.run(['patchelf','--set-rpath','$ORIGIN',str(target)],check=True)
            sys.stdout.buffer.write(target.read_bytes())
        else:
            main()
    except (OSError, RuntimeError, subprocess.SubprocessError, ValueError) as error:
        print('Forge isolated build failed: '+str(error),file=sys.stderr)
        sys.exit(1)
