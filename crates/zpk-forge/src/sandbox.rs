use crate::{fail, files::append, hash, Result};
use std::{
    io::{Read, Write},
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};
pub const IMAGE: &str = "zionos-forge:0.1.0";
pub const ALPINE_IMAGE: &str =
    "alpine@sha256:ff7f0c727dd796aa36787bfcd9a64bf35a6ce72a73451b3359fde849b6703d63";
const DOCKERFILE: &str = include_str!("../builder/Dockerfile");
const DRIVER: &str = include_str!("../builder/driver.py");
pub fn builder_hash() -> String {
    hash(format!("{DOCKERFILE}\0{DRIVER}").as_bytes())
}
pub fn run(
    program: &str,
    args: &[String],
    input: Option<Vec<u8>>,
    seconds: u64,
    limit: usize,
) -> Result<Vec<u8>> {
    let mut child = Command::new(program)
        .args(args)
        .stdin(if input.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("cannot start trusted tool {program}: {e}; no host build fallback"))?;
    let out = child.stdout.take().ok_or("missing stdout pipe")?;
    let err = child.stderr.take().ok_or("missing stderr pipe")?;
    fn capture(mut stream: impl Read, limit: usize) -> std::io::Result<(Vec<u8>, bool)> {
        let mut bytes = Vec::new();
        let mut buffer = [0u8; 16384];
        let mut overflow = false;
        loop {
            let n = stream.read(&mut buffer)?;
            if n == 0 {
                break;
            }
            let keep = n.min(limit.saturating_sub(bytes.len()));
            bytes.extend_from_slice(&buffer[..keep]);
            overflow |= keep < n;
        }
        Ok((bytes, overflow))
    }
    let stdout = thread::spawn(move || capture(out, limit));
    let stderr = thread::spawn(move || capture(err, 1024 * 1024));
    let writer = if let Some(bytes) = input {
        let mut pipe = child.stdin.take().ok_or("missing stdin pipe")?;
        Some(thread::spawn(move || pipe.write_all(&bytes)))
    } else {
        None
    };
    let deadline = Instant::now() + Duration::from_secs(seconds);
    let mut timed_out = false;
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if Instant::now() >= deadline {
            timed_out = true;
            let _ = child.kill();
            break child.wait()?;
        }
        thread::sleep(Duration::from_millis(50));
    };
    let (output, overflow) = stdout.join().map_err(|_| "stdout reader panicked")??;
    let (errors, error_overflow) = stderr.join().map_err(|_| "stderr reader panicked")??;
    if let Some(writer) = writer {
        let _ = writer.join();
    }
    if timed_out {
        return fail(format!("isolated tool deadline exceeded ({seconds}s)"));
    }
    if !status.success() {
        return fail(format!(
            "isolated command failed: {}; build output tail: {}",
            String::from_utf8_lossy(&errors).escape_debug(),
            String::from_utf8_lossy(&output[output.len().saturating_sub(4096)..]).escape_debug()
        ));
    }
    if overflow || error_overflow {
        return fail("isolated tool output limit exceeded");
    }
    Ok(output)
}
pub fn docker(
    args: &[&str],
    input: Option<Vec<u8>>,
    seconds: u64,
    limit: usize,
) -> Result<Vec<u8>> {
    run(
        "docker",
        &args.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
        input,
        seconds,
        limit,
    )
}
pub fn local_daemon() -> Result<()> {
    let endpoint = if let Some(context) = std::env::var("DOCKER_CONTEXT")
        .ok()
        .filter(|c| !c.is_empty())
    {
        String::from_utf8(docker(
            &[
                "context",
                "inspect",
                &context,
                "--format",
                "{{.Endpoints.docker.Host}}",
            ],
            None,
            15,
            4096,
        )?)?
        .trim()
        .to_owned()
    } else if let Ok(host) = std::env::var("DOCKER_HOST") {
        host
    } else {
        String::from_utf8(docker(
            &[
                "context",
                "inspect",
                "--format",
                "{{.Endpoints.docker.Host}}",
            ],
            None,
            15,
            4096,
        )?)?
        .trim()
        .to_owned()
    };
    if !endpoint.starts_with("unix:///") {
        return fail("Forge only accepts a local Unix Docker endpoint; remote/TCP/SSH builders are unsupported");
    }
    Ok(())
}
pub fn prepare() -> Result<()> {
    local_daemon()?;
    // Explicit trusted setup only; also supplies the minimal runtime image used by opt-in tests.
    docker(&["pull", ALPINE_IMAGE], None, 300, 1024 * 1024)?;
    let mut context = tar::Builder::new(Vec::new());
    append(&mut context, "Dockerfile", DOCKERFILE.as_bytes(), 0o644, 0)?;
    append(&mut context, "driver.py", DRIVER.as_bytes(), 0o644, 0)?;
    let label = format!("org.zionos.forge.source={}", builder_hash());
    docker(
        &["build", "--label", &label, "--tag", IMAGE, "-"],
        Some(context.into_inner()?),
        1200,
        1024 * 1024,
    )?;
    Ok(())
}
pub fn image_id() -> Result<String> {
    local_daemon()?;
    let bytes=docker(&["image","inspect",IMAGE],None,15,128*1024).map_err(|_|"trusted Forge builder is missing; run build --prepare-builder (trusted toolchain download only)")?;
    let values: Vec<serde_json::Value> = serde_json::from_slice(&bytes)?;
    let image = values.first().ok_or("missing builder metadata")?;
    if image["Config"]["Labels"]["org.zionos.forge.source"].as_str()
        != Some(builder_hash().as_str())
        || image["Architecture"] != "amd64"
    {
        return fail("builder provenance/architecture mismatch; prepare the trusted builder again");
    }
    Ok(image["Id"]
        .as_str()
        .ok_or("missing immutable builder ID")?
        .to_owned())
}
pub fn container_args(name: &str, image: &str) -> Vec<String> {
    [
        "create",
        "--name",
        name,
        "--network",
        "none",
        "--read-only",
        "--user",
        "10000:10000",
        "--cap-drop",
        "ALL",
        "--security-opt",
        "no-new-privileges:true",
        "--pids-limit",
        "128",
        "--cpus",
        "2",
        "--memory",
        "1g",
        "--memory-swap",
        "1g",
        "--ulimit",
        "nofile=256:256",
        "--ulimit",
        "core=0",
        "--tmpfs",
        "/work:rw,exec,nosuid,nodev,size=512m,mode=1777",
        "--tmpfs",
        "/tmp:rw,noexec,nosuid,nodev,size=64m,mode=1777",
        "--log-driver",
        "none",
        "--hostname",
        "zion-forge",
        image,
    ]
    .into_iter()
    .map(str::to_owned)
    .collect()
}
pub struct Container {
    name: String,
}
impl Container {
    pub fn create(image: &str, source: Vec<u8>) -> Result<Self> {
        let name = format!(
            "zpk-forge-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_nanos()
        );
        run("docker", &container_args(&name, image), None, 20, 4096)?;
        let container = Self { name };
        docker(&["start", &container.name], None, 20, 4096)?;
        docker(
            &["exec", &container.name, "/bin/mkdir", "-p", "/work/input"],
            None,
            15,
            4096,
        )?;
        // Only our canonical regular-file snapshot is extracted, inside tmpfs, before source execution.
        docker(
            &[
                "exec",
                "-i",
                &container.name,
                "/bin/tar",
                "-xf",
                "-",
                "-C",
                "/work/input",
            ],
            Some(source),
            30,
            4096,
        )?;
        Ok(container)
    }
    pub fn build(&self) -> Result<serde_json::Value> {
        docker(
            &[
                "exec",
                &self.name,
                "/usr/bin/python3",
                "/usr/local/lib/forge-driver.py",
            ],
            None,
            600,
            1024 * 1024,
        )?;
        Ok(serde_json::from_slice(
            &self.cat("/work/toolchains.json", 64 * 1024)?,
        )?)
    }
    pub fn export(&self) -> Result<Vec<u8>> {
        docker(
            &[
                "exec",
                &self.name,
                "/bin/tar",
                "-C",
                "/work/export",
                "-cf",
                "-",
                ".",
            ],
            None,
            30,
            crate::files::MAX_TOTAL as usize,
        )
    }
    fn cat(&self, path: &str, limit: usize) -> Result<Vec<u8>> {
        docker(&["exec", &self.name, "/bin/cat", path], None, 15, limit)
    }
    pub fn library(&self, name: &str) -> Result<Vec<u8>> {
        if !crate::elf::library_name(name) {
            return fail("unsafe dynamic library name");
        }
        docker(&["exec",&self.name,"/usr/bin/python3","/usr/local/lib/forge-driver.py","--library",name],None,30,crate::files::MAX_FILE as usize)
            .map_err(|_|format!("required library {name} is unavailable in trusted Alpine builder; declare/vendor it explicitly").into())
    }
}
impl Drop for Container {
    fn drop(&mut self) {
        let _ = docker(&["rm", "--force", &self.name], None, 20, 4096);
    }
}
