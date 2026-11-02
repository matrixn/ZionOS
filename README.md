# ZionOS Genesis 0.6.1 — NAS experimental

ZionOS este un sistem NAS live/diskless: Alpine Linux 3.23, kernel Alpine LTS,
OpenRC, API Rust/Axum și desktop în browser. **Nu este pregătit pentru producție.**
Configurația live nu persistă după reboot. API-ul nu are autentificare și trebuie
folosit numai în QEMU izolat, prin forwarding local `127.0.0.1:8080`.

**0.6.1 suspendă `zion-lab-provision --apply`.** Planul și diagnosticul rămân
read-only; API-ul nu autorizează scrieri. Revalidarea shell din 0.6 nu elimina
cursa dintre verificare și formatare. Un executor exclusiv verificat este cerință
pentru 0.7. [Raportul 0.6.1](docs/GENESIS_061.md) descrie remedierile și limitele.

## Checklist cumulativ: Genesis 0.1–0.6.1

Istoricul a fost reconciliat cu conversația inițială „Utilitatea unui OS multiplatformă”
și cu sursele actuale. `[x]` indică implementare prezentă; verificările sunt raportate
separat. Nu presupune că toate scenariile hardware au fost validate.

### Genesis 0.1 — fundația bootabilă

- [x] Profil ISO Alpine 3.23 pentru x86-64 și kernel LTS furnizat de Alpine.
- [x] OpenRC: `sysinit`, `boot`, `default`, shutdown; `eth0` cu DHCP și loopback.
- [x] Serviciu `zion-core` executat ca `nobody:nobody` și `GET /api/health`.
- [x] Staging Rust musl, build ISO Docker și launcher QEMU.
- [x] API local implicit; acces guest numai prin contractul explicit QEMU izolat.

### Genesis 0.2 — Hardware Dashboard

- [x] Dashboard static la `/` și JavaScript la `/app.js`.
- [x] `GET /api/metrics`: CPU, RAM, uptime, hostname, kernel, arhitectură și discuri.
- [x] Actualizare la 5 secunde și istoric de 60 de eșantioane CPU.
- [x] Teste unitare pentru parsarea și calculul metricilor.

### Genesis 0.3 — Desktop Edition

- [x] Desktop, dock, launcher cu căutare și ferestre mobile/minimizabile/maximizabile.
- [x] System Overview, Activity Monitor, Network, Settings și About.
- [x] Light/dark, accente, preferințe locale și reducerea animațiilor; layout responsive.
- [x] JavaScript nativ, fără biblioteci externe la runtime.
- [x] File Station indică explicit un preview, fără acces la fișiere.
- [ ] Migrare Vue/TypeScript: există numai scaffold, nefolosit de ISO.

### Genesis 0.4 — Storage Manager

- [x] `GET /api/storage`: discuri fizice/logice, partiții, mount-uri și sector sizes.
- [x] Filesystem/UUID/label prin probe read-only, când permisiunile permit.
- [x] Capacitate/spațiu disponibil pentru mount-uri și SMART JSON când este accesibil.
- [x] Timeout pentru procesele de probe și oprirea lor la anulare.
- [x] Detalii pe disc; datele API sunt afișate prin `textContent`.
- [ ] SMART/blkid complet pe hardware real: serviciul neprivilegiat poate primi acces refuzat.

### Genesis 0.5 — Storage Pools & Volumes

- [x] `GET /api/pools`: observarea MD RAID și Btrfs din sysfs.
- [x] Membri MD, nivel, stare, degradare și sincronizare raportate de kernel.
- [x] Volume montate/nemontate; capacitatea filesystemului este distinctă de cea a partiției.
- [x] Capacity Lab Single/Mirror/RAID5: estimare în browser și copierea rezumatului.
- [x] Candidații sunt numai pentru simulare; nu certifică eligibilitatea pentru ștergere.
- [ ] Creare pool Single/RAID1 printr-un motor Rust structurat.

### Genesis 0.6 — Provisioning & Recovery Lab

- [x] `GET /api/provisioning`: preflight parțial și recomandări read-only.
- [x] Launcher cu QCOW2 separat pentru `/dev/vdb` în QEMU.
- [x] `zion-lab-provision --plan`: validare fără scriere.
- [ ] Calea CLI `--apply` din 0.6 este suspendată în 0.6.1 până la un executor sigur.
- [x] Refuz pentru vendor non-QEMU, dimensiuni nepermise, partiții, mount-uri,
  holders și semnături detectate; revalidare înaintea operației manuale.
- [x] `zion-lab-recover --check`: `e2fsck -fn`, fără reparații automate.
- [x] Provisioning Lab și Recovery Center în desktop; fără scrieri HTTP.
- [x] Branding issue/motd/zionos-release și explicații colorate pentru etapele OpenRC.
- [ ] Validare distructivă end-to-end `--apply`: neexecutată în acest audit.
- [ ] Restaurare, rollback, jurnal persistent și recuperare automată.

## Actualizări din auditul 2026-11-02

- [x] Un singur wrapper pre-OpenRC; codul duplicat de după arhivare eliminat.
- [x] Verificări pentru executabile și assets înaintea generării overlay-ului.
- [x] Branding avansat în `/init` din initramfs, după shebang, păstrând corpul Alpine.
- [x] Banner numai cu builtins; refuz pentru injectare duplicată sau init incompatibil.
- [x] Builderul extrage initramfs-ul din ISO și verifică `/init` executabil și bannerul.
- [x] CPU: fără dublarea timpului guest; contoarele invalide/overflow sunt refuzate.
- [x] SMART distinge standby de erorile de acces prin `standby,3`.
- [x] Proba header-ului lab refuză citiri eșuate/incomplete și verifică exact 4096 bytes.
- [x] Invocări Bash explicite, inclusiv în checkout-uri fără executable bit.
- [x] Launchere cu `exec`, pentru oprirea corectă a procesului QEMU la finalul testului.
- [x] Smoke test: refuză portul ocupat; `-snapshot` evită persistența în discul virtual.
- [x] Smoke test verifică ordinea initramfs → OpenRC → DHCP → Zion Core.
- [x] API smoke verifică refuzul POST pentru toate cele cinci rute API existente.
- [x] ISO `zionos-genesis-0.6.0-x86_64.iso`, ales determinist de launcher.
- [x] Backup-ul depășit `scripts/build-iso.sh1` eliminat.
- [x] Politică LF pentru OpenRC/lab; CI verifică toate scripturile Alpine.
- [x] 7 teste noi pentru header, overlay, initramfs și port ocupat.

Brandingul initramfs precede OpenRC; mesajele bootloader-ului și kernelului pot apărea
înaintea lui. Bannerul anunță etapele următoare, nu succesul lor. `/etc/os-release`
păstrează identitatea Alpine. Health și provisioning raportează `0.6.1`;
storage păstrează schema `0.4.0`, pools `0.5.0`, cu câmpuri suplimentare explicite.

## Consolă animată — actualizare după audit

- [x] Logo ASCII mare ZIONOS, paletă neon și indicator de activitate animat în OpenRC.
- [x] Etape reale `sysinit` → `boot` → `default`, fără procent inventat sau întârzieri artificiale.
- [x] Ultimele mesaje reale în panou; loguri complete în `/var/log/zion-boot/` și scrollback serial.
- [x] Stare online condiționată de health API; stare pending distinctă.
- [x] Oprirea rendererului și restaurarea cursorului după fiecare etapă; erorile rămân vizibile.
- [x] Mod clasic cu `zion.splash=off`, fallback fără TTY, terminal mic sau `TERM=dumb`.
- [x] 13 teste fixture, inclusiv PTY, eroare, cursor, întrerupere și ready/pending.
- [x] ISO reconstruit; QEMU implicit și `Conroe`: boot, DHCP, API și finalizare ready verificate.

Intro-ul din initramfs este static; animația acoperă OpenRC, inclusiv verificarea modloop.
Detalii, mod verbose, loguri și limite: [Consola de boot](docs/BOOT_CONSOLE.md).

## Genesis 0.6.1 — siguranță și performanță

- [x] Guard Rust comun API/CLI: identitate de sesiune, VirtIO, block device și major:minor,
  capacitate, read-only/removable, partiții, holders/slaves, mount-uri și swap.
- [x] Erori de inventar/probe refuzate; aliasurile mount/swap nu se compară prin substring.
- [x] Launcher lab cu QCOW2 nou la fiecare pornire, serial aleator și poziții PCI stabile.
- [x] Probe privilegiate pe descriptor read-only exclusiv; header exact și erori blkid
  distincte de semnătură absentă; recovery cere ext4 și rulează numai `e2fsck -fn`.
- [x] API pe loopback implicit; QEMU necesită contract fw_cfg, peer gateway permis,
  verificări Host/Origin și limită de cereri concurente. Acestea nu înlocuiesc autentificarea.
- [x] Cache comun storage/pools de 5s, SMART de 30s, preflight lab de 3s;
  o colectare în zbor și bugete globale. Un worker blocat nu este duplicat.
- [x] UI elimină informația lab expirată la erori, unește cererile simultane și
  suspendă polling-ul periodic când tabul este ascuns; storage și când fereastra este minimizată.
- [x] Capacity Lab: validarea numărului de discuri, dublurilor și overflow-ului; teste JS.
- [x] Cargo cache pe filesystem WSL nativ; builder Alpine la digest și aports la commit fixat.
- [x] Cache ISO invalidat prin conținut/permisiuni/cheie publică; manifest SHA256/provenance.
- [x] ISO validat înainte de publicare atomică: rebuild-ul nu rescrie imaginea deschisă de VM.
- [x] KVM automat când poate crea o VM; fallback TCG; parametrii de disc/rețea nu pot fi suprascriși.
- [ ] Auth/TLS, rate limiting per client, executor de formatare, restaurare și teste fizice.

## ZPK — TASK 00, fundație pentru pachete native

- [x] Crate separat `crates/zpk-schema`: schema v1 Serde/TOML, validare manifest și CLI read-only.
- [x] Arhivă USTAR + zstd cu SHA-256, inventar exact, limite de expansiune și refuz linkuri/căi nesigure.
- [x] Metadate pentru executabile musl, dependențe, serviciu opțional, zero/una/mai multe intrări desktop,
  ferestre, iconuri PNG, assets, permisiuni, date/config și compatibilitate de upgrade.
- [x] Exemple valide, fixture-uri invalide, teste de arhive malițioase și CI separat.
- [x] Verificate local: 23 teste ZPK, Clippy strict, regresiile Genesis, build ISO și boot QEMU nou.
- [x] Exemplu nativ hello compilat și împachetat; inspectorul nu execută conținutul.
- [ ] Instalare, repository, semnături, sandbox și integrare în desktop/ISO: neimplementate.

**Hash-urile nu autentifică editorii. ZPK nesemnat este numai pentru dezvoltare locală de încredere.**
Genesis 0.6.1 și comenzile sale rămân neschimbate; ZPK nu este inclus în ISO.
Nu introducem încă workspace Cargo la rădăcină: păstrăm lockfile-ul și build-ul Core,
iar noul crate are lockfile propriu. Detalii: [documentația ZPK](docs/zpk/README.md),
[format v1](docs/zpk/FORMAT_V1.md), [verificări](docs/zpk/VERIFICATION.md).

```sh
bash tests/zpk/check.sh
bash examples/zpk-hello/build-package.sh
cargo run --locked --manifest-path crates/zpk-schema/Cargo.toml --bin zpk-inspect -- \
  build/zpk/hello-1.0.0.zpk 0.6.1
```

## ZPK Forge — TASK 01

- [x] CLI `zpk forge init/build/package/inspect/validate/clean` în `crates/zpk-forge`.
- [x] Build-uri reale Rust/Cargo, C/C++ CMake, Go pur și Make cu staging declarativ.
- [x] Builder Alpine separat, fără mount-uri host/credențiale/discuri, UID neprivilegiat,
  rețea oprită, root read-only și limite de resurse; fără fallback la compilare host.
- [x] `init` citește sursele fără execuție și raportează câmpurile ambigue `UNRESOLVED`.
- [x] Staging temporar, colectare recursivă de biblioteci musl, verificare ELF/DT_NEEDED
  și RUNPATH relativ; împachetare offline, hash-uri și provenance.
- [x] Exemple `zion-hello-rust` și `zion-hello-c`, plus teste reale Go/Make/C++.
- [x] Rebuild-uri identice byte-for-byte și execuție a exemplelor în Alpine minimal,
  fără compilatoare sau runtime OCI cerut de aplicație.
- [x] 16 teste offline Forge și 2 teste Docker; Foundation și Genesis păstrate.
- [ ] aarch64/i686/ARMv7, CGO, Zig/Meson/Autotools, verificare completă ISA,
  instalare și integrare în ISO: nesuportate în acest task.

Docker este numai mediul de BUILD/test. Sursele pot executa cod arbitrar în builder;
izolarea folosește kernelul Docker și nu oferă garanția unei VM dedicate. Flagurile
x86-64 și proprietățile ELF sunt verificate, dar nu certifică toate instrucțiunile
executate pe Core 2 Duo. Aceste limite sunt afișate la build/package și în provenance.

```bash
export CARGO_TARGET_DIR="${XDG_CACHE_HOME:-$HOME/.cache}/zionos/zpk-target"
cargo build --locked --manifest-path crates/zpk-forge/Cargo.toml
"$CARGO_TARGET_DIR/debug/zpk" forge build --project examples/zion-hello-rust --prepare-builder
"$CARGO_TARGET_DIR/debug/zpk" forge package --project examples/zion-hello-rust
"$CARGO_TARGET_DIR/debug/zpk" forge inspect examples/zion-hello-rust/.zpk-forge/org.zionos.zion-hello-rust-1.0.0.zpk
bash tests/zpk/forge-check.sh --docker
```

La repetare, alege `package --output /director/existent/nou.zpk` sau folosește
`forge clean --project ...` pentru a elimina numai rezultatele generate. Nu se
suprascriu pachete existente. [Comenzi complete, manifest și limite](docs/zpk/FORGE.md)
· [Verificări executate](docs/zpk/FORGE_VERIFICATION.md).

## Native ZPK Runtime — TASK 02

- [x] `crates/zpk-runtime`: model de configurare administrator, lifecycle nativ și adaptor OpenRC extensibil.
- [x] Start/stop/restart/status, health de proces sau Unix socket, logs și autostart;
  respawn și oprire de grup prin `supervise-daemon`, fără supervisor global nou.
- [x] Servicii background, backend comun pentru mai multe intrări desktop și model
  on-demand/static fără daemon permanent obligatoriu.
- [x] Versiuni root-owned în `/opt/zionos/apps/<id>/versions`, referință `current`,
  date/config/loguri separate și directoare private cu identități dedicate neprivilegiate.
- [x] Landlock ABI >=6 obligatoriu, seccomp x86-64, capabilități eliminate, no_new_privs
  și limite rlimit; acces NAS/dispozitive/shell/IP și IPC neautorizat refuzat în VM.
- [x] 13 teste unitare cu OpenRC mock și integrare nativă QEMU: lifecycle, crash budget,
  CPU/memorie/FD/procese/fișiere, sănătate Unix, logs, autostart și date păstrate la restart.
- [x] `zpk-common` partajează acum inspecția ELF între Forge și Runtime, fără Docker în runtime.
- [ ] Broker autentificat, activare UI din desktop, cgroup-uri agregate, cote filesystem,
  instalare/upgrade și persistență după reboot: neimplementate/nevalidate în acest task.

Aplicațiile rulează ca procese Linux native: **fără Docker/OCI/Kubernetes la runtime**.
Restricțiile implementate nu sunt o izolare echivalentă unei VM; declarațiile de
permisiuni nu acordă singure acces OS. ISO-ul live nu presupune persistență și runtime-ul
nu este adăugat automat în el. [Arhitectură, comenzi și proprietăți](docs/zpk/RUNTIME.md)
· [Verificări](docs/zpk/RUNTIME_VERIFICATION.md).

```bash
bash tests/zpk/runtime-check.sh
# După ce există ISO-ul Genesis; aplicațiile rulează numai în VM temporară:
bash tests/zpk/runtime-check.sh --vm
```

## Build și teste Genesis: Windows + WSL2

Rulează din Ubuntu WSL cu integrarea Docker Desktop activă. Ai nevoie de Rust,
rustfmt/Clippy, target musl, `musl-tools`, Docker, QEMU, `qemu-img`, curl, Python 3 și Node.js.

```bash
cd /mnt/c/Users/radut/Workspace/zionos
rustup target add x86_64-unknown-linux-musl
cargo fmt --manifest-path core/api/Cargo.toml --check
cargo test --manifest-path core/api/Cargo.toml
cargo clippy --manifest-path core/api/Cargo.toml --all-targets -- -D warnings
bash tests/check-safety.sh
node --check web/desktop/app.js
node --test tests/desktop.test.cjs
bash scripts/build-core.sh
bash scripts/build-iso.sh
```

Build-ul ISO folosește un container privilegiat de încredere. Cache-ul și cheia privată
locală APK rămân în volumul Docker `zionos-build-cache-<workspace-id>`, în afara Git;
cheia publică intră în imagine. Nu șterge volumul dacă vrei să păstrezi cheia/cache-ul.
Cargo folosește `~/.cache/zionos/<workspace-id>/cargo`, cu override `CARGO_TARGET_DIR`.
ISO, manifest, QCOW2 și loguri rămân în `build/`, ignorate de Git. Pachetele APK sunt
încă furnizate din repository mobil: pinning-ul builder/aports nu face build-ul
reproductibil byte-for-byte și manifestul nu este un SBOM complet al sistemului live.

Măsurat local: prima populare a cache-ului ISO **280s**, rebuild cu cache **21–25s**,
cu aceeași imagine de aproximativ **352 MiB**, fără eliminarea firmware-ului/modulelor.
Boot și smoke: **36s KVM**, **159s TCG/Conroe**. Prima încercare TCG a avut un
kernel panic timpuriu; reluarea aceleiași imagini a trecut. Cauza rămâne de investigat,
vezi raportul 0.6.1; compatibilitatea pe hardware fizic nu este certificată.

## Rulare și verificare ISO

```bash
# Automat: boot, branding, DHCP, API și assets; fără provisioning distructiv.
bash scripts/test-vm.sh
# Compatibilitate ISA Core 2 Duo prin emulare strictă, fără KVM.
ZION_QEMU_ACCEL=tcg bash scripts/test-vm.sh -cpu Conroe
# Interactiv, cu fereastră QEMU.
bash scripts/run-qemu.sh
# Alternativ: numai consolă, fără GUI.
bash scripts/run-qemu.sh -display none
# Forțează accelerarea (eșuează explicit dacă KVM nu este accesibil).
ZION_QEMU_ACCEL=kvm bash scripts/run-qemu.sh -display none
```

Oprește VM-ul înainte de a porni următorul: toate folosesc portul 8080.
În WSL, utilizatorul `matrixn` a fost adăugat cu autorizare în grupul `kvm`;
terminalele existente pot necesita redeschidere sau `newgrp kvm`. Launcherul afișează
acceleratorul ales. KVM accelerează rularea, dar nu validează emularea ISA Conroe.
În consola serială, `Ctrl+A`, apoi `X` închide QEMU. Pentru un ISO ales explicit:

```bash
ZION_ISO=/cale/absoluta/zionos.iso bash scripts/run-qemu.sh
```

Din alt terminal, cu VM-ul pornit:

```bash
bash tests/smoke.sh
curl -fsS http://127.0.0.1:8080/api/health
curl -fsS http://127.0.0.1:8080/api/provisioning
```

Din PowerShell: `curl.exe http://localhost:8080/api/health`.
Desktop: [http://localhost:8080](http://localhost:8080). În Storage Manager verifică
inventarul, poolurile, Capacity Lab, Provisioning Lab și Recovery Center.
IP-ul guest `10.0.2.15` este în rețeaua QEMU; Windows folosește forwarding-ul local.

Inspecție nedistructivă a laboratorului:

```bash
bash scripts/run-qemu-lab.sh
# În consola guest, numai pe discul virtual dedicat:
zion-lab-provision --plan
# Automat, separat de VM-ul interactiv:
bash scripts/test-vm.sh --lab
python3 tests/lab-console.py
```

Nu folosi disk passthrough sau discuri fizice. Header-ul gol nu dovedește că întregul
disc este nou. `inspection_ready` indică numai preflight parțial, nu eligibilitate
pentru scrieri; `execution_enabled` și `eligible_for_lab` sunt permanent `false`.
Diagnosticul `zion-lab-recover --check` refuză discul lab gol: cere ext4 confirmat.
Nu rula mai multe VM-uri simultan. QCOW2-urile lab sunt păstrate în `build/disks/lab.*`
pentru inspecție și nu sunt refolosite automat.

## Rezultate și roadmap

- [x] Rust: 33 teste, rustfmt și Clippy cu `-D warnings`.
- [x] 16 teste de siguranță/consolă/cache pe fixture-uri, fără dispozitive bloc.
- [x] 3 teste JavaScript: capacitate, erori lab și cereri simultane.
- [x] Sintaxă Bash, shell Alpine/OpenRC și `node --check` pentru desktopul activ.
- [x] Build release musl și staging.
- [x] Build final ISO și verificarea `/init` din imaginea produsă.
- [x] QEMU implicit și cu CPU `Conroe`: branding timpuriu, OpenRC, DHCP, API/assets și refuz POST.
- [x] Scaffold Vue: `npm run build` trece în WSL; avertisment pentru scriptul static nebundlat.
- [ ] Validare vizuală interactivă a desktopului și teste pe hardware fizic.
- [ ] Teste distructive CLI, repair/rollback, RAID1 real: neexecutate.

**0.6.1:** guard comun, apply suspendat, izolarea API, bugete/cache și build/KVM mai rapide.
**0.7 P1:** planuri Rust Single/RAID1, executor exclusiv cu identitate fixată și dovezi
de recuperare; **P2:** fault injection, rezultate recovery exportabile, wizard și
snapshot-uri APK/SBOM. Web rămâne read-only. [Criterii și limite](docs/GENESIS_061.md).

[Audit, fișiere modificate, riscuri și criterii de acceptare](docs/AUDIT_GENESIS_06.md).

La fiecare lot coerent: actualizează checklist-ul și raportul, rulează verificările
și build-ul aplicabil, apoi commit și push pe ramura de lucru autorizată. Nu bifa
verificări neexecutate și nu face merge automat.
