use std::{fs, io::Write, path::Path, process::Command};
use zpk_forge::{
    config::{self, Project, System},
    elf, files, sandbox, workflow,
};
const RUST: &str = include_str!("../../examples/zion-hello-rust/zpk.toml");
const C: &str = include_str!("../../examples/zion-hello-c/zpk.toml");
fn project() -> tempfile::TempDir {
    let r = tempfile::tempdir().unwrap();
    for (name, text) in [
        ("zpk.toml", RUST),
        ("Cargo.toml", "[package]\nname='hello'\nversion='1.0.0'\n"),
        ("Cargo.lock", "version = 4\n"),
        ("LICENSE", "MIT fixture\n"),
    ] {
        fs::write(r.path().join(name), text).unwrap();
    }
    fs::create_dir(r.path().join("src")).unwrap();
    fs::write(r.path().join("src/main.rs"), "fn main() {}\n").unwrap();
    r
}
fn fixture_elf() -> Vec<u8> {
    let mut b = vec![0u8; 128];
    b[..7].copy_from_slice(b"\x7fELF\x02\x01\x01");
    b[16] = 2;
    b[18] = 62;
    b[20] = 1;
    b[24..32].copy_from_slice(&0x400078u64.to_le_bytes());
    b[32] = 64;
    b[52] = 64;
    b[54] = 56;
    b[56] = 1;
    b[64] = 1;
    b[68] = 5;
    b[80..88].copy_from_slice(&0x400000u64.to_le_bytes());
    b[96] = 128;
    b[104] = 128;
    b[120] = 0xc3;
    b
}
fn staged(root: &Path, m: &zpk_schema::Manifest) {
    let p = root.join(&m.files[0].path);
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    fs::write(p, fixture_elf()).unwrap();
    fs::write(root.join("LICENSE"), "MIT fixture\n").unwrap();
}
#[test]
fn target_adapters_are_explicit() {
    assert!(zpk_forge::target::resolve("x86_64").is_ok());
    for t in ["aarch64", "i686", "armv7", "unknown"] {
        assert!(zpk_forge::target::resolve(t).is_err());
    }
}
#[test]
fn invalid_recipes_and_unresolved_fields_fail() {
    for s in [
        RUST.replace("system = \"rust\"", "system = \"zig\""),
        RUST.replace("version = 1\n", "version = 2\n"),
        RUST.replace("jobs = 2", "jobs = 0"),
        RUST.replace("build/forge-app", "../../host"),
        RUST.replace(
            "license_file = \"LICENSE\"",
            "license_file = \"/etc/passwd\"",
        ),
        RUST.replace("cargo_bin = \"zion-hello-rust\"", "cargo_bin = \"--bad\""),
        RUST.replace("[forge]", "[forge]\nunknown = true"),
        RUST.replace("org.zionos.zion-hello-rust", "UNRESOLVED"),
    ] {
        assert!(Project::parse(&s).is_err());
    }
    Project::parse(RUST).unwrap();
    Project::parse(C).unwrap();
    let mut p = Project::parse(RUST).unwrap();
    p.recipe.stage.push(p.recipe.stage[0].clone());
    assert!(p.recipe.validate(&p.manifest).is_err());
    p.recipe.stage.clear();
    assert!(p.recipe.validate(&p.manifest).is_err());
}
#[test]
fn detection_does_not_execute_or_guess_ambiguous_projects() {
    let r = project();
    fs::remove_file(r.path().join("zpk.toml")).unwrap();
    fs::write(
        r.path().join("Makefile"),
        "$(shell touch SHOULD_NOT_EXIST)\n",
    )
    .unwrap();
    assert_eq!(config::detect(r.path()).len(), 2);
    assert!(config::init(r.path(), None).is_err());
    assert!(!r.path().join("SHOULD_NOT_EXIST").exists());
    let missing = config::init(r.path(), Some(System::Rust)).unwrap();
    assert!(missing.contains(&"package.id".to_owned()));
    assert!(Project::load(r.path()).is_err());
    assert!(config::init(r.path(), Some(System::Rust)).is_err());
}
#[test]
fn init_reports_multiple_cargo_binaries_and_unknown_cmake_paths() {
    let r = project();
    fs::remove_file(r.path().join("zpk.toml")).unwrap();
    fs::write(
        r.path().join("Cargo.toml"),
        "[package]\nname='hello'\nversion='1.0.0'\n[[bin]]\nname='other'\npath='other.rs'\n",
    )
    .unwrap();
    assert!(config::init(r.path(), None)
        .unwrap()
        .iter()
        .any(|s| s == "forge.cargo_bin"));
    let r = tempfile::tempdir().unwrap();
    fs::write(r.path().join("CMakeLists.txt"), "project(hello)\n").unwrap();
    assert!(config::init(r.path(), None)
        .unwrap()
        .iter()
        .any(|s| s == "forge.stage[0].from"));
    let large = project();
    fs::remove_file(large.path().join("zpk.toml")).unwrap();
    fs::File::create(large.path().join("Cargo.toml"))
        .unwrap()
        .set_len(1024 * 1024 + 1)
        .unwrap();
    assert!(config::init(large.path(), None)
        .unwrap_err()
        .to_string()
        .contains("bounded regular file"));
}
#[test]
fn snapshots_copy_only_explicit_files_and_fingerprint_modes() {
    let r = project();
    let p = Project::parse(RUST).unwrap();
    fs::write(r.path().join(".env"), "must never be copied").unwrap();
    let a = files::snapshot(r.path(), &p.recipe, b"{}").unwrap();
    let b = files::snapshot(r.path(), &p.recipe, b"{}").unwrap();
    assert_eq!(a.tar, b.tar);
    assert!(!a.hashes.contains_key(".env"));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(
            r.path().join("src/main.rs"),
            fs::Permissions::from_mode(0o755),
        )
        .unwrap();
        assert_ne!(
            a.hashes,
            files::snapshot(r.path(), &p.recipe, b"{}").unwrap().hashes
        );
    }
    for s in [
        ".env",
        ".ssh/config",
        ".docker/config.json",
        ".npmrc",
        "src/key.pem",
        "../secret",
    ] {
        assert!(!config::source_path(s));
    }
}
#[test]
fn snapshots_refuse_missing_lock_overlap_and_oversized_sources() {
    let r = project();
    let mut p = Project::parse(RUST).unwrap();
    p.recipe.inputs.push("src/main.rs".into());
    assert!(files::snapshot(r.path(), &p.recipe, b"").is_err());
    p.recipe.inputs.pop();
    fs::remove_file(r.path().join("Cargo.lock")).unwrap();
    assert!(files::snapshot(r.path(), &p.recipe, b"").is_err());
    fs::write(r.path().join("Cargo.lock"), "lock").unwrap();
    fs::File::create(r.path().join("src/huge"))
        .unwrap()
        .set_len(files::MAX_FILE + 1)
        .unwrap();
    assert!(files::snapshot(r.path(), &p.recipe, b"").is_err());
}
#[cfg(unix)]
#[test]
fn symlink_ancestors_and_hardlinks_are_never_followed() {
    let r = project();
    let outside = tempfile::tempdir().unwrap();
    fs::write(outside.path().join("secret"), "fixture").unwrap();
    std::os::unix::fs::symlink(outside.path(), r.path().join("linked")).unwrap();
    let mut p = Project::parse(RUST).unwrap();
    p.recipe.inputs.push("linked/secret".into());
    assert!(files::snapshot(r.path(), &p.recipe, b"").is_err());
    p.recipe.inputs.pop();
    fs::hard_link(
        outside.path().join("secret"),
        r.path().join("src/linked.rs"),
    )
    .unwrap();
    assert!(files::snapshot(r.path(), &p.recipe, b"").is_err());
}
#[test]
fn sandbox_has_no_mounts_devices_network_or_privilege_fallback() {
    let a = sandbox::container_args("test", "sha256:test");
    for s in [
        "none",
        "--read-only",
        "10000:10000",
        "--cap-drop",
        "ALL",
        "no-new-privileges:true",
        "--pids-limit",
        "--memory",
        "--cpus",
        "--tmpfs",
    ] {
        assert!(a.contains(&s.to_owned()));
    }
    for s in [
        "--privileged",
        "--volume",
        "--mount",
        "--device",
        "--network=host",
        "/var/run/docker.sock",
    ] {
        assert!(!a.iter().any(|a| a.contains(s)));
    }
    assert!(sandbox::run("zpk-nonexistent-tool", &[], None, 1, 64)
        .unwrap_err()
        .to_string()
        .contains("no host build fallback"));
}
#[test]
fn trusted_tool_deadlines_output_limits_and_failures() {
    for (args, seconds, limit, message) in [
        (vec!["-c".into(), "exit 7".into()], 1, 128, "failed"),
        (
            vec!["-c".into(), "printf 'too much data'".into()],
            1,
            2,
            "output limit",
        ),
        (vec!["-c".into(), "exec sleep 5".into()], 0, 2, "deadline"),
    ] {
        assert!(sandbox::run("sh", &args, None, seconds, limit)
            .unwrap_err()
            .to_string()
            .contains(message));
    }
}
#[test]
fn staging_rejects_links_traversal_duplicates_and_oversize() {
    for kind in *b"123x" {
        let mut h = tar::Header::new_ustar();
        h.set_path("payload/bin/bad").unwrap();
        h.set_size(0);
        h.set_mode(0o644);
        h.set_entry_type(tar::EntryType::new(kind));
        h.set_cksum();
        let mut b = h.as_bytes().to_vec();
        b.resize(1536, 0);
        assert!(files::read_stage(&b, tempfile::tempdir().unwrap().path()).is_err());
    }
    let mut t = tar::Builder::new(Vec::new());
    files::append(&mut t, "payload/bin/app", b"x", 0o755, 0).unwrap();
    files::append(&mut t, "payload/bin/app", b"x", 0o755, 0).unwrap();
    assert!(files::read_stage(
        &t.into_inner().unwrap(),
        tempfile::tempdir().unwrap().path()
    )
    .is_err());
    let mut h = tar::Header::new_ustar();
    h.set_size(files::MAX_FILE + 1);
    h.set_mode(0o644);
    h.set_path("payload/bin/large").unwrap();
    h.set_cksum();
    let mut b = h.as_bytes().to_vec();
    b.resize(1536, 0);
    assert!(files::read_stage(&b, tempfile::tempdir().unwrap().path()).is_err());
    let mut h = tar::Header::new_ustar();
    h.set_size(0);
    h.set_mode(0o644);
    h.as_mut_bytes()[..100].fill(0);
    h.as_mut_bytes()[..9].copy_from_slice(b"../escape");
    h.set_cksum();
    let mut b = h.as_bytes().to_vec();
    b.resize(1536, 0);
    assert!(files::read_stage(&b, tempfile::tempdir().unwrap().path()).is_err());
}
#[test]
fn deterministic_archive_and_staging_inventory_validation() {
    let p = Project::parse(RUST).unwrap();
    let s = tempfile::tempdir().unwrap();
    staged(s.path(), &p.manifest);
    let a = workflow::package_bytes(s.path(), &p.manifest).unwrap();
    assert_eq!(a, workflow::package_bytes(s.path(), &p.manifest).unwrap());
    let report = workflow::inspect_package(&a).unwrap();
    assert!(!report.package.authenticated);
    assert!(report.dependencies[0].needed.is_empty());
    fs::write(s.path().join("extra"), "undeclared").unwrap();
    assert!(workflow::package_bytes(s.path(), &p.manifest).is_err());
    fs::remove_file(s.path().join("extra")).unwrap();
    fs::write(s.path().join(&p.manifest.files[0].path), b"not ELF").unwrap();
    assert!(workflow::package_bytes(s.path(), &p.manifest).is_err());
}
#[test]
fn elf_machine_dependency_names_and_isa_requirements() {
    let mut b = fixture_elf();
    b[18] = 183;
    assert!(elf::inspect("payload/bin/app", &b)
        .unwrap_err()
        .to_string()
        .contains("unsupported ELF architecture"));
    for s in [
        "../../host",
        "/lib/x.so",
        "libbad.so/../../x",
        "libbad.so;cmd",
    ] {
        assert!(!elf::library_name(s));
    }
    assert!(elf::library_name("libstdc++.so.6"));
    let mut p = vec![];
    p.extend_from_slice(&0xc0008002u32.to_le_bytes());
    p.extend_from_slice(&4u32.to_le_bytes());
    p.extend_from_slice(&1u32.to_le_bytes());
    p.extend_from_slice(&0u32.to_le_bytes());
    elf::check_properties(&p).unwrap();
    p[8] = 4;
    assert!(elf::check_properties(&p)
        .unwrap_err()
        .to_string()
        .contains("unsupported CPU"));
    assert!(elf::check_properties(&[0]).is_err());
}
#[test]
fn clean_checks_marker_and_symlinks_preserving_project_source() {
    let r = project();
    fs::create_dir(r.path().join(".zpk-forge")).unwrap();
    fs::write(r.path().join(".zpk-forge/precious"), "keep").unwrap();
    assert!(workflow::clean(r.path()).is_err());
    assert!(r.path().join(".zpk-forge/precious").exists());
    fs::remove_dir_all(r.path().join(".zpk-forge")).unwrap();
    workflow::state(r.path(), true).unwrap();
    workflow::clean(r.path()).unwrap();
    assert!(r.path().join("src/main.rs").exists());
    #[cfg(unix)]
    {
        let outside = tempfile::tempdir().unwrap();
        std::os::unix::fs::symlink(outside.path(), r.path().join(".zpk-forge")).unwrap();
        assert!(workflow::clean(r.path()).is_err());
        assert!(outside.path().exists());
        outside.close().unwrap();
        assert!(workflow::clean(r.path()).is_err());
    }
}
fn state_fixture(root: &Path) -> std::path::PathBuf {
    let p = Project::load(root).unwrap();
    let state = workflow::state(root, true).unwrap();
    let generation = state.join("gen-fixture");
    fs::create_dir(&generation).unwrap();
    let stage = generation.join("stage");
    fs::create_dir(&stage).unwrap();
    staged(&stage, &p.manifest);
    let text = toml::to_string(&p.manifest).unwrap();
    fs::write(generation.join("manifest.toml"), &text).unwrap();
    let report = workflow::Provenance {
        forge_version: "0.1.0".into(),
        builder_image: "sha256:test".into(),
        builder_source: sandbox::builder_hash(),
        source_files: files::snapshot(root, &p.recipe, b"").unwrap().hashes,
        authoring_sha256: zpk_forge::hash(p.authoring.as_bytes()),
        resolved_manifest_sha256: zpk_forge::hash(text.as_bytes()),
        toolchains: serde_json::json!({}),
        staged_files: files::stage_hashes(&stage).unwrap(),
        dependencies: vec![],
        platform_libraries: vec![],
        cpu_verification: elf::CPU_LIMIT.into(),
        isolation: "fixture".into(),
    };
    fs::write(
        generation.join("provenance.json"),
        serde_json::to_vec(&report).unwrap(),
    )
    .unwrap();
    fs::write(
        state.join("current.json"),
        r#"{"generation":"gen-fixture"}"#,
    )
    .unwrap();
    generation
}
#[test]
fn offline_package_rejects_stale_source_manifest_and_stage() {
    for field in ["source", "manifest", "stage"] {
        let r = project();
        let g = state_fixture(r.path());
        match field {
            "source" => fs::write(r.path().join("src/main.rs"), "changed").unwrap(),
            "manifest" => {
                writeln!(
                    fs::OpenOptions::new()
                        .append(true)
                        .open(r.path().join("zpk.toml"))
                        .unwrap(),
                    "# changed"
                )
                .unwrap();
            }
            _ => fs::write(g.join("stage/LICENSE"), "changed").unwrap(),
        };
        assert!(workflow::package(r.path(), None)
            .unwrap_err()
            .to_string()
            .contains("changed"));
    }
    let r = project();
    state_fixture(r.path());
    let a = r.path().join("a.zpk");
    let b = r.path().join("b.zpk");
    workflow::package(r.path(), Some(&a)).unwrap();
    workflow::package(r.path(), Some(&b)).unwrap();
    assert_eq!(fs::read(&a).unwrap(), fs::read(&b).unwrap());
    assert!(workflow::package(r.path(), Some(&a)).is_err());
}
#[test]
fn cli_inspection_is_offline_and_installation_is_absent() {
    assert!(!Command::new(env!("CARGO_BIN_EXE_zpk"))
        .args(["forge", "install"])
        .output()
        .unwrap()
        .status
        .success());
    let p = Project::parse(RUST).unwrap();
    let r = tempfile::tempdir().unwrap();
    staged(r.path(), &p.manifest);
    let path = r.path().join("x.zpk");
    fs::write(
        &path,
        workflow::package_bytes(r.path(), &p.manifest).unwrap(),
    )
    .unwrap();
    assert!(Command::new(env!("CARGO_BIN_EXE_zpk"))
        .args(["forge", "inspect"])
        .arg(path)
        .env("DOCKER_HOST", "tcp://forbidden:2375")
        .output()
        .unwrap()
        .status
        .success());
}

#[test]
fn remote_build_daemons_are_rejected_before_any_source_execution() {
    let r = project();
    let output = Command::new(env!("CARGO_BIN_EXE_zpk"))
        .args(["forge", "build", "--project"])
        .arg(r.path())
        .env("DOCKER_CONTEXT", "")
        .env("DOCKER_HOST", "tcp://forbidden:2375")
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("local Unix Docker endpoint"));
    assert!(!r.path().join(".zpk-forge").exists());
}

fn runtime_check(generation: &Path, manifest: &zpk_schema::Manifest) {
    let mut archive = tar::Builder::new(Vec::new());
    for file in &manifest.files {
        files::append(
            &mut archive,
            &file.path,
            &fs::read(generation.join("stage").join(&file.path)).unwrap(),
            if file.kind == zpk_schema::manifest::FileKind::Executable {
                0o755
            } else {
                0o644
            },
            10000,
        )
        .unwrap();
    }
    let executable = format!(
        "/work/{}",
        manifest
            .files
            .iter()
            .find(|f| f.kind == zpk_schema::manifest::FileKind::Executable)
            .unwrap()
            .path
    );
    // Only the owned hello fixtures are executed, in a second minimal Alpine sandbox with no toolchains.
    let out = sandbox::docker(
        &[
            "run",
            "--rm",
            "--pull=never",
            "-i",
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
            "32",
            "--memory",
            "128m",
            "--memory-swap",
            "128m",
            "--cpus",
            "1",
            "--tmpfs",
            "/work:rw,exec,nosuid,nodev,size=64m,mode=1777",
            "--entrypoint",
            "/bin/sh",
            sandbox::ALPINE_IMAGE,
            "-c",
            r#"tar -xf - -C /work && exec "$1""#,
            "forge-runtime-check",
            &executable,
        ],
        Some(archive.into_inner().unwrap()),
        30,
        4096,
    )
    .unwrap();
    assert!(String::from_utf8_lossy(&out)
        .to_lowercase()
        .contains("hello"));
}
fn copy_example(name: &str) -> tempfile::TempDir {
    fn copy(s: &Path, d: &Path) {
        fs::create_dir_all(d).unwrap();
        for e in fs::read_dir(s).unwrap() {
            let e = e.unwrap();
            if e.file_name() == ".zpk-forge" {
                continue;
            }
            let target = d.join(e.file_name());
            if e.file_type().unwrap().is_dir() {
                copy(&e.path(), &target);
            } else {
                fs::copy(e.path(), target).unwrap();
            }
        }
    }
    let r = tempfile::tempdir().unwrap();
    copy(
        &Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples")
            .join(name),
        r.path(),
    );
    r
}
#[test]
#[ignore = "requires prepared trusted Docker builder; all compilation is isolated"]
fn real_rust_cmake_go_make_cpp_and_full_build_reproducibility() {
    sandbox::image_id().unwrap();
    for system in ["rust", "cmake", "go", "make", "cpp"] {
        let r = copy_example(if system == "rust" {
            "zion-hello-rust"
        } else {
            "zion-hello-c"
        });
        match system {
            "go" => {
                fs::write(
                    r.path().join("go.mod"),
                    "module example.org/hello\n\ngo 1.23\n",
                )
                .unwrap();
                fs::write(
                    r.path().join("main.go"),
                    "package main\nimport \"fmt\"\nfunc main(){fmt.Println(\"hello Go\")}\n",
                )
                .unwrap();
                let text = RUST
                    .replace("system = \"rust\"", "system = \"go\"")
                    .replace(
                        "inputs = [\"Cargo.toml\", \"Cargo.lock\", \"src\", \"LICENSE\"]",
                        "inputs = [\"go.mod\", \"main.go\", \"LICENSE\"]",
                    )
                    .replace("cargo_bin = \"zion-hello-rust\"", "go_package = \".\"");
                fs::write(r.path().join("zpk.toml"), text).unwrap();
            }
            "make" => {
                fs::write(
                    r.path().join("Makefile"),
                    "all:\n\t$(CC) $(CFLAGS) hello.c -o hello\n",
                )
                .unwrap();
                let text = C
                    .replace("system = \"cmake\"", "system = \"make\"")
                    .replace(
                        "inputs = [\"CMakeLists.txt\", \"hello.c\", \"LICENSE\"]",
                        "inputs = [\"Makefile\", \"hello.c\", \"LICENSE\"]",
                    )
                    .replace("jobs = 2", "jobs = 2\nmake_target = \"all\"")
                    .replace("dest/usr/bin/zion-hello-c", "source/hello");
                fs::write(r.path().join("zpk.toml"), text).unwrap();
            }
            "cpp" => {
                fs::write(r.path().join("CMakeLists.txt"),"cmake_minimum_required(VERSION 3.20)\nproject(hello LANGUAGES CXX)\nadd_executable(zion-hello-c hello.c)\nset_source_files_properties(hello.c PROPERTIES LANGUAGE CXX)\ninstall(TARGETS zion-hello-c RUNTIME DESTINATION bin)\n").unwrap();
                fs::write(
                    r.path().join("hello.c"),
                    "#include <iostream>\nint main(){std::cout << \"hello C++\";}\n",
                )
                .unwrap();
            }
            _ => {}
        }
        let generation = workflow::build(r.path(), false).unwrap();
        let a = r.path().join("a.zpk");
        workflow::package(r.path(), Some(&a)).unwrap();
        let report = workflow::inspect_package(&fs::read(&a).unwrap()).unwrap();
        runtime_check(&generation, &report.package.manifest);
        assert_eq!(report.package.manifest.target.architecture, "x86_64");
        if system == "cpp" {
            assert!(report
                .package
                .manifest
                .files
                .iter()
                .any(|f| f.path.contains("libstdc++")));
            let reduced = tempfile::tempdir().unwrap();
            let mut manifest = report.package.manifest.clone();
            manifest
                .files
                .retain(|f| f.kind != zpk_schema::manifest::FileKind::Library);
            for file in &manifest.files {
                let to = reduced.path().join(&file.path);
                fs::create_dir_all(to.parent().unwrap()).unwrap();
                fs::copy(generation.join("stage").join(&file.path), to).unwrap();
            }
            fs::copy(
                generation.join("stage/LICENSE"),
                reduced.path().join("LICENSE"),
            )
            .unwrap();
            let archive = workflow::package_bytes(reduced.path(), &manifest).unwrap();
            assert!(workflow::inspect_package(&archive)
                .unwrap_err()
                .to_string()
                .contains("unresolved dynamic dependency"));
        }
        let p: serde_json::Value =
            serde_json::from_slice(&fs::read(generation.join("provenance.json")).unwrap()).unwrap();
        assert!(p["toolchains"]["isolation_checks"]
            .as_str()
            .unwrap()
            .contains("passed"));
        workflow::build(r.path(), false).unwrap();
        let b = r.path().join("b.zpk");
        workflow::package(r.path(), Some(&b)).unwrap();
        assert_eq!(
            fs::read(a).unwrap(),
            fs::read(b).unwrap(),
            "nonreproducible {system}"
        );
    }
}
#[test]
#[ignore = "requires Docker; failed compiler/build/staging stays isolated"]
fn real_missing_compiler_failed_build_and_staging_errors() {
    for scenario in ["missing", "failed", "staging"] {
        let r = copy_example("zion-hello-c");
        match scenario{"missing"=>fs::write(r.path().join("CMakeLists.txt"),"cmake_minimum_required(VERSION 3.20)\nset(CMAKE_C_COMPILER /nonexistent-compiler)\nproject(fail LANGUAGES C)\n").unwrap(),"failed"=>fs::write(r.path().join("hello.c"),"intentional invalid C").unwrap(),_=>fs::write(r.path().join("zpk.toml"),C.replace("dest/usr/bin/zion-hello-c","dest/usr/bin/missing")).unwrap()};
        assert!(workflow::build(r.path(), false).is_err());
        assert!(!r.path().join(".zpk-forge/current.json").exists());
    }
}
