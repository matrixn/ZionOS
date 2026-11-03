# ZionOS Genesis 0.7 — Native Package Center

ZionOS este un sistem NAS live/diskless: Alpine Linux 3.23, kernel Alpine LTS,
OpenRC, API Rust/Axum și desktop în browser. **Nu este pregătit pentru producție.**
Sistemul de bază rămâne live/diskless. Aplicațiile, contul administratorului și
registrul persistă numai pe volumul ext4 configurat explicit prin UUID.
Package Center are autentificare, sesiuni și CSRF; API-urile NAS rămân read-only.
Administrarea este limitată la fluxul local/QEMU `127.0.0.1:8080`, fără acces LAN.

**0.6.1 suspendă `zion-lab-provision --apply`.** Planul și diagnosticul rămân
read-only; API-ul nu autorizează scrieri. Revalidarea shell din 0.6 nu elimina
cursa dintre verificare și formatare. Un executor exclusiv verificat este cerință
înaintea oricărei activări viitoare. Genesis 0.7 păstrează această protecție.
[Raportul 0.6.1](docs/GENESIS_061.md) descrie remedierile și limitele.

## Genesis 0.7 — Package Center funcțional

- [x] ISO cu Manager, Runtime, broker și catalog local `zion-hello` compilat prin Forge.
- [x] Volum persistent selectat prin UUID, validat înaintea montării; mod degradat
  explicit pentru volum absent/invalid sau registru corupt, fără instalare în RAM.
- [x] Administrator creat prin bootstrap local, Argon2id, sesiuni persistente,
  expirare/logout, Origin/Host, CSRF și autorizare verificată independent în broker.
- [x] Core dedicat neprivilegiat; broker root cu IPC Unix versionat, peer credentials,
  descriptori read-only, acțiuni limitate și jurnal/audit; fără shell arbitrar.
- [x] Discover, Installed, căutare/filtrare, upload real limitat, preview și acord
  explicit pentru permisiuni și încredere locală în arhive nesemnate.
- [x] Grid responsive de carduri cu icon/inițială, versiune și stare; click/Enter/Space
  deschide pagina de detalii. Install/Uninstall și Start/Stop depind de starea reală.
- [x] Install, Start, Stop, Restart, status/loguri, upgrade, rollback executabil și
  uninstall cu păstrarea datelor; progres din fazele reale Manager/OpenRC.
- [x] Launcher actualizat fără reload, icon/window/SDK, teme și notificări;
  revocarea assets după upgrade/uninstall, fără icon fictiv pentru servicii fără UI.
- [x] Teste reale browser/QEMU pentru reboot, upgrade eșuat, întreruperea tranzacției,
  refuz auth/CSRF/IPC, arhive malițioase și spațiu insuficient.
- [x] Runner interactiv verificat pe TCG/Conroe; `--resume` păstrează contul și aplicația.
- [x] Opt teste pentru crearea sigură a imaginilor/refuzuri resume; șapte joburi CI
  trecute pentru integrarea `c6499d5`, raportate separat de workflow-ul VM manual.
- [ ] Administrare LAN/HTTPS, cookie Secure și configurare proxy de producție.
- [ ] Semnături/catalog remote, RBAC/MFA/reset parolă, cote agregate și purge date.
- [ ] Broker general pentru fișiere/settings SDK; acestea rămân `UNAVAILABLE`.

```sh
cd /mnt/c/Users/radut/Workspace/zionos
bash scripts/build-iso.sh
bash scripts/run-qemu-package-center.sh
# Repornește exact mediul păstrat de prima comandă:
bash scripts/run-qemu-package-center.sh --resume build/center-XXXXXX
```

Deschide **http://127.0.0.1:8080 → Package Center**. Codul inițial este în fișierul
privat `setup-code.txt` din directorul afișat; alegi parola, nu există una implicită.
Comanda `quit` oprește VM-ul curat. Nu rula simultan alt server/VM pe portul 8080.
Fără `--resume` se creează alt mediu gol; nu se formatează un disc existent.

[Ghid complet și comenzi](docs/GENESIS_07.md) ·
[Arhitectură și limite de securitate](docs/GENESIS_07_ARCHITECTURE.md) ·
[Rezultate executate și capturi reale](docs/GENESIS_07_VERIFICATION.md).

## Checklist cumulativ: Genesis 0.1–0.7


Istoricul a fost reconciliat cu conversația inițială „Utilitatea unui OS multiplatformă”
și cu sursele actuale. `[x]` indică implementare prezentă; verificările sunt raportate
separat. Nu presupune că toate scenariile hardware au fost validate.

### Genesis 0.1 — fundația bootabilă

- [x] Profil ISO Alpine 3.23 pentru x86-64 și kernel LTS furnizat de Alpine.
- [x] OpenRC: `sysinit`, `boot`, `default`, shutdown; `eth0` cu DHCP și loopback.
- [x] Serviciu `zion-core` neprivilegiat și `GET /api/health` (cont dedicat din 0.7).
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
La TASK 00 ZPK nu era inclus în ISO. Integrarea reală în ISO și Package Center
este livrată în Genesis 0.7; comenzile istorice de validare rămân compatibile.
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

## ZPK End-to-End Integration — TASK 05

- [x] Source → Forge → validare `.zpk` → Manager local → OpenRC non-root → launcher/fereastră.
- [x] Trei booturi curate cu același QCOW2: registry, UID și setări native păstrate.
- [x] Upgrade real `1.0.0` → `1.1.0`; pornirea eșuată a candidatului `1.2.0` restaurează executabilul anterior.
- [x] Întreruperi reale ale Manager: instalare staged și upgrade în activare, recuperat după reboot.
- [x] Ferestre/assets retrase la upgrade/uninstall; datele rămân după dezinstalare.
- [x] Setare nativă bounded, scrisă atomic de backend; acces local prin socket Unix privat.
- [x] Mediu interactiv separat și imagini noi în `build/`; niciun disc fizic, fără `-snapshot` în testul persistent.
- [x] Același scenariu complet verificat pe CPU Conroe emulat/TCG, separat de KVM; hardware Core 2 fizic neverificat.
- [ ] Setări editabile din UI/SDK: necesită broker autentificat; răspunsul actual rămâne `UNAVAILABLE`.
- [ ] Persistență automată de producție în ISO-ul live; bootstrap-ul E2E este exclusiv pentru VM-ul de test.

[Ghid complet build/install/launch/upgrade](docs/zpk/E2E.md),
[raport tehnic și verificări](docs/zpk/E2E_VERIFICATION.md).

```sh
bash tests/e2e/check.sh                  # teste rapide și regresii SDK
bash tests/e2e/check.sh --packages       # trei versiuni compilate prin Forge
bash scripts/build-iso.sh
bash scripts/test-vm.sh --zpk-e2e        # trei booturi persistente + Chromium
bash tests/e2e/check.sh --legacy-vm      # același scenariu, CPU Conroe/TCG
bash tests/e2e/check.sh --interactive    # aplicație instalată; localhost:8080
```

Comenzile necesită toolchain/builder/Chromium pregătite conform ghidului. Pornirea
normală `bash scripts/run-qemu.sh` păstrează ISO-ul live cu aplicațiile built-in.

## Zion App SDK & Native Desktop Integration — TASK 04

- [x] Registry dinamic built-in + ZPK instalate/activate; `GET /api/apps` read-only.
- [x] Publicare Manager atomică, verificată SHA-256; resurse limitate la UI/assets declarate.
- [x] Ferestre comune cu drag/resize/min/max, instanțe multiple, dock/pin, căutare și tastatură.
- [x] SDK JavaScript/ESM + TypeScript: titlu, dimensiuni, temă, notificări și confirmări.
- [x] Iframe cu origine opacă, CSP, handshake versionat și verificarea sursei/sesiunii/permisiunilor.
- [x] `zion-hello`: backend Rust nativ compilat prin Forge, UI și icon; OpenRC non-root.
- [x] ISO final, Chromium desktop/mobile și instalare/dezinstalare într-un QEMU dedicat verificate.
- [ ] Fișiere/settings SDK: răspuns tipizat `UNAVAILABLE`; brokerul autentificat rămâne viitor.
- [ ] Autentificare Core, origini dedicate aplicațiilor și izolare completă a rețelei browserului.

Implementare și comenzi: [App SDK](docs/zpk/APP_SDK.md),
[verificări executate](docs/zpk/APP_SDK_VERIFICATION.md),
[exemplul Zion Hello](examples/zion-hello/README.md).
ISO-ul live păstrează built-in-urile; instalarea necesită volume persistente și CLI-ul
local Manager/Runtime pregătit. Nu există instalare sau pornire privilegiată prin HTTP.

```sh
npm --prefix web/zion-app-sdk ci --ignore-scripts --no-audit --no-fund
bash tests/apps/check.sh
# Cu Chromium, builderul Forge și toolchain-urile pregătite conform ghidului:
bash tests/apps/check.sh --browser
bash scripts/build-iso.sh
bash tests/apps/check.sh --vm
# Pornire normală a ISO-ului live:
bash scripts/run-qemu.sh
```

## Native Package Manager — TASK 03

- [x] `crates/zpk-manager`: CLI local privilegiat, registru SQLite separat de datele aplicațiilor și lock de concurență.
- [x] Install/list/info/start/stop/restart/status/logs/upgrade/rollback/uninstall/verify și `repair --plan`.
- [x] Tranzacții `PREPARED → STAGED → ACTIVATING → COMMITTED`, stări FAILED/RECOVERY_REQUIRED,
  fsync, publicare fără suprascriere, compensare și reconciliere după întrerupere.
- [x] Validare și extracție prin Foundation, identități/servicii native prin Runtime;
  resolver offline pentru versiuni, dependențe lipsă/cicluri/conflicte și dependențe inverse.
- [x] Politică explicită pentru pachete locale nesemnate și aprobarea exactă a permisiunilor;
  filesystem-urile live/RAM sunt refuzate pentru instalare persistentă.
- [x] Versiuni alăturate și rollback de executabile; date/config/loguri și identități păstrate la uninstall.
- [x] Metadate pentru datele păstrate: UID/GID și schema rămân rezervate după uninstall/reboot;
  reinstalarea incompatibilă este refuzată.
- [x] Modele UI static/on-demand/backend comun; selecție explicită pentru executabile desktop ambigue.
- [x] Exemplele Forge Rust/C au mod foreground `--serve`, socket Unix și contor persistent;
  rularea obișnuită fără argumente rămâne print-and-exit.
- [x] Teste de instalator cu servicii mock, 46 de întreruperi reale de proces și test QEMU
  cu trei boot-uri pe același QCOW2 dedicat, inclusiv reinstalare în ordine inversă.
- [x] Broker/UI autentificat și catalog local în Genesis 0.7.
- [ ] Catalog semnat, migrări de date, ștergerea datelor,
  activări multi-pachet și garanții la pierderea alimentării: neimplementate/nevalidate.

Nu există endpoint-uri HTTP de instalare, instalare pe hostul Windows/WSL sau runtime Docker.
Managerul nu este adăugat automat în ISO. Persistența și configurația de boot trebuie
pregătite separat. [Arhitectură și comenzi](docs/zpk/MANAGER.md)
· [Verificări și limite](docs/zpk/MANAGER_VERIFICATION.md).

```bash
bash tests/zpk/manager-check.sh
# Necesită ISO-ul Genesis, builderul Forge pregătit, QEMU/musl/xorriso/e2fsprogs:
bash tests/zpk/manager-check.sh --vm
```

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
- [x] Broker autentificat și activare UI din desktop în Genesis 0.7.
- [ ] Cgroup-uri agregate, cote filesystem,
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

Măsurători istorice 0.6.1: prima populare a cache-ului ISO **280s**, rebuild cu cache **21–25s**,
cu aceeași imagine de aproximativ **352 MiB**, fără eliminarea firmware-ului/modulelor.
Boot și smoke: **36s KVM**, **159s TCG/Conroe**. Prima încercare TCG a avut un
kernel panic timpuriu; reluarea aceleiași imagini a trecut. Cauza rămâne de investigat,
vezi raportul 0.6.1; compatibilitatea pe hardware fizic nu este certificată.
Pentru ISO-ul 0.7 cu Forge/catalog/helper-e, măsurarea cached este aproximativ 79s
și 358 MiB; [raportul curent](docs/GENESIS_07_VERIFICATION.md) separă rezultatele.

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

## Rezultate și roadmap — istoricul auditului 0.6.1

- [x] Rust: 33 teste, rustfmt și Clippy cu `-D warnings`.
- [x] 16 teste de siguranță/consolă/cache pe fixture-uri, fără dispozitive bloc.
- [x] 3 teste JavaScript: capacitate, erori lab și cereri simultane.
- [x] Sintaxă Bash, shell Alpine/OpenRC și `node --check` pentru desktopul activ.
- [x] Build release musl și staging.
- [x] Build final ISO și verificarea `/init` din imaginea produsă.
- [x] QEMU implicit și cu CPU `Conroe`: branding timpuriu, OpenRC, DHCP, API/assets și refuz POST.
- [x] Scaffold Vue: `npm run build` trece în WSL; avertisment pentru scriptul static nebundlat.
- [x] Validare browser real dark/light/mobil în Genesis 0.7, raportată separat mai sus.
- [ ] Teste pe hardware fizic.
- [ ] Teste distructive CLI, repair/rollback, RAID1 real: neexecutate.

**0.6.1:** guard comun, apply suspendat, izolarea API, bugete/cache și build/KVM mai rapide.
**Storage ulterior, separat de Package Center 0.7:** planuri Rust Single/RAID1, executor exclusiv cu identitate fixată și dovezi
de recuperare; **P2:** fault injection, rezultate recovery exportabile, wizard și
snapshot-uri APK/SBOM. API-ul storage rămâne read-only. [Criterii și limite](docs/GENESIS_061.md).

[Audit, fișiere modificate, riscuri și criterii de acceptare](docs/AUDIT_GENESIS_06.md).

La fiecare lot coerent: actualizează checklist-ul și raportul, rulează verificările
și build-ul aplicabil, apoi commit și push pe ramura de lucru autorizată. Nu bifa
verificări neexecutate și nu face merge automat.
