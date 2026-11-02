# ZionOS Genesis 0.6 — NAS experimental

ZionOS este un sistem NAS live/diskless: Alpine Linux 3.23, kernel Alpine LTS,
OpenRC, API Rust/Axum și desktop în browser. **Nu este pregătit pentru producție.**
Configurația live nu persistă după reboot. API-ul nu are autentificare și trebuie
folosit numai în QEMU izolat, prin forwarding local `127.0.0.1:8080`.

## Checklist cumulativ: Genesis 0.1–0.6

Istoricul a fost reconciliat cu conversația inițială „Utilitatea unui OS multiplatformă”
și cu sursele actuale. `[x]` indică implementare prezentă; verificările sunt raportate
separat. Nu presupune că toate scenariile hardware au fost validate.

### Genesis 0.1 — fundația bootabilă

- [x] Profil ISO Alpine 3.23 pentru x86-64 și kernel LTS furnizat de Alpine.
- [x] OpenRC: `sysinit`, `boot`, `default`, shutdown; `eth0` cu DHCP și loopback.
- [x] Serviciu `zion-core` executat ca `nobody:nobody` și `GET /api/health`.
- [x] Staging Rust musl, build ISO Docker și launcher QEMU.
- [x] API pe interfața guest; forwarding host limitat la localhost.

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
- [x] Cale CLI manuală `--apply`: ext4 numai pe vdb, verificări și confirmare interactivă.
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
păstrează identitatea Alpine. Versiunile schemelor API rămân compatibile: health și
provisioning `0.6.0`, storage `0.4.0`, pools `0.5.0`.

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

## Build și teste: Windows + WSL2

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
bash scripts/build-core.sh
bash scripts/build-iso.sh
```

Build-ul ISO folosește un container privilegiat de încredere; cheile temporare de
semnare APK rămân în container. ISO, QCOW2 și loguri rămân în `build/`, ignorate de Git.
Ramura Alpine `3.23-stable` și pachetele nu sunt fixate la digest/versiune exactă;
build-ul nu este încă reproductibil byte-for-byte.

## Rulare și verificare ISO

```bash
# Automat: boot, branding, DHCP, API și assets; fără provisioning distructiv.
bash scripts/test-vm.sh
# Același test cu CPU emulat din familia Core 2 Duo.
bash scripts/test-vm.sh -cpu Conroe
# Interactiv, cu fereastră QEMU.
bash scripts/run-qemu.sh
# Alternativ: numai consolă, fără GUI.
bash scripts/run-qemu.sh -display none
```

Oprește VM-ul înainte de a porni următorul: toate folosesc portul 8080.
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
```

Nu folosi disk passthrough sau discuri fizice. Vendorul QEMU și header-ul gol nu
dovedesc că întregul disc este nou; vezi riscurile rămase în raport.

## Rezultate și roadmap

- [x] Rust: 19 teste, rustfmt și Clippy cu `-D warnings`.
- [x] 13 teste de siguranță și consolă pe fixture-uri, fără dispozitive bloc.
- [x] Sintaxă Bash, shell Alpine/OpenRC și `node --check` pentru desktopul activ.
- [x] Build release musl și staging.
- [x] Build final ISO și verificarea `/init` din imaginea produsă.
- [x] QEMU implicit și cu CPU `Conroe`: branding timpuriu, OpenRC, DHCP, API/assets și refuz POST.
- [x] Scaffold Vue: `npm run build` trece în WSL; avertisment pentru scriptul static nebundlat.
- [ ] Validare vizuală interactivă a desktopului și teste pe hardware fizic.
- [ ] Teste distructive CLI, repair/rollback, RAID1 real: neexecutate.

**0.6.1:** siguranța CLI, acoperirea testelor, bugetul probelor și izolarea API.
**0.7:** planuri Rust Single/RAID1, identități stabile, simulare și diagnostic structurat;
execuție numai în lab, web în continuare read-only.

[Audit, fișiere modificate, riscuri și criterii de acceptare](docs/AUDIT_GENESIS_06.md).

La fiecare lot coerent: actualizează checklist-ul și raportul, rulează verificările
și build-ul aplicabil, apoi commit și push pe ramura de lucru autorizată. Nu bifa
verificări neexecutate și nu face merge automat.
