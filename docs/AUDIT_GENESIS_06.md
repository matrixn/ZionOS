# Audit Genesis 0.6 și roadmap

Data: 2026-11-02. Branch: `codex/genesis-06-audit`.
Scop: audit de inginerie pe sursele locale, remedieri limitate, branding initramfs
solicitat ulterior, teste, build și publicarea ramurii autorizate. Nu este o
certificare de securitate sau un release Genesis 0.7.

## Comportament verificat în surse

- API Rust/Axum: numai GET/HEAD pentru health, metrics, storage, pools și provisioning;
  două assets statice, fără cale de fișier controlată de client. Fără autentificare.
- OpenRC rulează API-ul ca `nobody:nobody`; endpointul provisioning citește sysfs,
  nu apelează utilitarul privilegiat. Nu am mărit privilegiile serviciului.
- Inventarul citește sysfs/proc, `statvfs`, `blkid -p` și SMART JSON; procesele au
  timeout și `kill_on_drop`. Pools folosește inventarul fără SMART.
- Lab-ul CLI validează numai `/dev/vdb`, vendor QEMU, markerul imaginii, dimensiuni,
  writable/removable, partiții, mount-uri, holders, blkid și header. Apply necesită
  TTY, frază exactă și o a doua validare. Nu a fost executat în audit.
- Recovery este `e2fsck -fn`, nu restaurare, repair sau rollback. API-ul furnizează
  recomandări și preflight, nu rezultatul executării unui control de integritate.
- Desktopul activ este HTML/CSS/JavaScript static; Vue/Vite este scaffold separat.
  Valorile API sunt afișate prin `textContent`; HTML interpolat provine din definiții
  locale de aplicații/iconuri. Nu am identificat un traseu API → `innerHTML` în această revizie.
- QEMU folosește user networking și forwarding pe `127.0.0.1`; nu disk passthrough.
  Bind-ul guest `0.0.0.0` este necesar forwarding-ului și a fost păstrat.

## Probleme confirmate și remedieri

| Prioritate | Problemă și efect | Rezultat |
|---|---|---|
| P1 | Pipeline-ul citirii header-ului ascundea erori/short reads; un rezultat gol putea trece controlul de zero | Statusul `dd` este verificat separat; lungime exactă și numai zero; fixture-uri pentru eșec/incomplet/nonzero |
| P1 | Smoke test putea accepta un API deja pornit pe 8080; procesul urmărit putea fi un shell părinte | Refuz port ocupat, `exec` până la QEMU, verificarea procesului înainte/după smoke, `-snapshot` |
| P2 | Două definiții wrapper/inittab, ultima după `tar` | Eliminat blocul inefectiv; verificări overlay și test de conținut arhivă |
| P2 | Apeluri directe către scripturi versionate 100644 | Invocare explicită cu Bash, funcțională în checkout Linux curat |
| P2 | `smartctl` exit 2 trata accesul refuzat ca standby | `-n standby,3`, parser separat, teste pentru acces/standby/health failure |
| P2 | Totalul `/proc/stat` aduna guest/guest_nice deja incluse în user/nice | Primele opt contoare și adunare verificată; teste guest/invalid/overflow |
| P2 | Tag ISO 0.1.0 și selectarea primului ISO puteau testa imaginea veche | Tag/nume 0.6.0 și selecție deterministă; `ZION_ISO` explicit pentru alte imagini |
| P3 | README 0.1 și documentație Vue generică descriau incomplet implementarea | Checklist cumulativ, separarea scaffold-ului, comenzi și limite actualizate |

Precizare: înaintea remedierii, **prima** definiție wrapper era deja inclusă corect
în arhivă la `/usr/local/sbin/zion-init-wrapper`. Blocul duplicat de după `tar` nu
rescria arhiva existentă; nu am confirmat că acel bloc producea singur un ISO nebootabil.

### Branding avansat cerut în timpul auditului

Builderul instalează mkinitfs în containerul efemer, apoi inserează bannerul după
shebang în `/usr/share/mkinitfs/initramfs-init`. Am inspectat implementările Alpine
`update-kernel` și `mkinitfs`: generatorul folosește acest fișier ca `/init`.
Corpul upstream este păstrat, cu verificare de sintaxă și refuzul injectării duble.
Nu schimbăm initramfs-ul mașinii gazdă, comanda kernelului, driver-ele sau runlevel-urile.

Bannerul folosește numai shell builtins și nu montează filesystemuri. Builderul
extrage initramfs-ul din ISO-ul efectiv și verifică bannerul și bitul executabil.
Testul QEMU verifică ordinea initramfs → wrapper OpenRC → DHCP → Zion Core.
Mesajele bootloader-ului/kernelului pot apărea anterior; explicațiile nu sunt un
traducător pentru fiecare mesaj de kernel/serviciu.

## Riscuri rămase, ordonate după prioritate

1. **P1 — Izolarea API este o cerință de deployment.** `main.rs` ascultă pe toate
   interfețele guest, fără auth/TLS/rate limiting. Host-forwarding-ul este local;
   boot pe hardware cu LAN/bridging ar putea expune hostname, topologie și metrici.
   Nu folosi ISO-ul pe o rețea publică. Soluția viitoare trebuie să păstreze accesul QEMU.
2. **P1 — Eligibilitatea lab nu dovedește un disc nou.** `zion-lab-provision`
   verifică vendor/marker, dar nu o identitate stabilă emisă pentru discul exact la
   lansare; toate ISO-urile au markerul. Un QCOW2 reutilizat cu date necunoscute în
   afara header-ului poate trece probele. Lipsesc swap/ancestry sistem, identificare
   robustă major:minor, locking și eliminarea completă a cursei dintre validare și mkfs.
3. **P1 — Unele verificări de inventar nu refuză sigur erorile.** În CLI,
   `find | grep`, citirea mountinfo prin grep și absența directorului holders pot
   interpreta lipsa dovezii ca absența utilizării. Verificarea prin substring nu
   acoperă sigur mount-uri prin alias sau configurații swap. Necesită fixture-uri și
   redesign limitat înainte de extinderea execuției; nu am pretins că fixul header rezolvă tot.
4. **P2 — Probe costisitoare, fără buget global.** `snapshot_inner` serializează
   până la 32 de discuri × 128 partiții, cu timeout blkid de 2s, plus SMART de 3s/disc.
   Limita teoretică pentru probe depășește două ore; browserul abandonează la 20/25s.
   Clienți multipli pot porni inventare duplicate. `statvfs` și citirile fs sunt sincrone
   în handlers async și pot bloca worker-ele pe dispozitive cu probleme.
5. **P2 — Inventarul nu reprezintă o autorizare de provisioning.** Probe blkid
   interpretează stdout fără verificarea exit status; există probe numai pe partiții,
   nu semnături complete pe discuri întregi. Mountinfo filtrează sursele `/dev/`,
   ceea ce poate omite aliasuri. API `eligible_for_lab` omite mount-uri, holders,
   removable, semnături și marker, deși UI îl etichetează „Parțial OK”.
6. **P2 — Permisiuni hardware și recovery limitate.** SMART/blkid pot fi inaccesibile
   pentru nobody; nu se rezolvă prin rularea întregului web server ca root. Recovery
   verifică mount-uri tot prin substring și nu furnizează raport persistat, reparare,
   reconstruire RAID sau restaurare. Jurnalul în RAM nu supraviețuiește reboot-ului.
7. **P2 — Build-uri variabile.** Container tag, ramură aports și pachete mobile;
   patch de compatibilitate aplicat condiționat pe text upstream, build privilegiat.
   Lipsesc pinning, provenance/SBOM, manifest cu hash-uri și matrice BIOS/UEFI.
8. **P3 — UI și acoperire.** Nu există teste DOM/browser. La eroare în `pollLab`,
   doar motivele sunt înlocuite; sumarul/recomandările anterioare pot rămâne vizibile.
   Polling-ul continuă în fundal/minimizat; proba storage și pools dublează blkid.
   Estimările Single/Mirror/RAID5 sunt formule în JS, fără teste, nu planuri de operații.

Acestea sunt limite deduse din sursă; nu sunt exploit-uri reproduse pe dispozitive
reale. Nu am testat formatare, repair, passthrough sau scriere pe blocuri fizice.

## Roadmap și criterii de acceptare

### Genesis 0.6.1 — stabilizarea baseline-ului

- [x] P1: verificarea citirii header-ului și izolarea smoke test-ului.
- [x] P2: wrapper unic, verificarea overlay/initramfs, branding timpuriu, selecția ISO.
- [x] P2: corecții CPU/SMART și verificări extinse în CI.
- [ ] P1: fixture-uri pentru fiecare refuz CLI: mount alias, swap, holders, partiții,
  probe eșuate, marker/vendor lipsă, identitate schimbată și sistem/boot ancestry.
- [ ] P1: refuz sigur pe erori de citire/inventar; legarea vdb de identitatea lab
  dedicată; validare consecventă între API/CLI, fără extinderea căilor distructive.
- [ ] P2: cache comun cu expirare, o singură colectare în zbor, concurență mică și
  buget global; filesystem I/O blocant izolat; niciun busy loop sau serviciu greu nou.
- [ ] P2: API tests pentru contracte/erori, probe mock, anulare și lipsă privilegii;
  fixture-uri mountinfo, MD degradat și semnături ambigue.
- [ ] P2: corectitudinea stării UI după timeout și teste de capacitate/DOM fără framework nou.
- [ ] P2: ISO reproducibil cu versiuni/digest-uri, manifest checksum și BIOS/UEFI smoke.

Acceptare: toate verificările existente și noile fixture-uri trec, QEMU boot/API
trece pentru CPU baseline, API rămâne read-only, niciun test automat nu execută mkfs/repair.

### Genesis 0.7 — Secure Storage Provisioning Engine

- [ ] P1: model Rust tipizat pentru dispozitiv/identitate/topologie și eligibilitate;
  respinge rezultate incomplete, discuri sistem, mount-uri, swap, holders și semnături.
- [ ] P1: plan/validate/simulate separate de executor; Single ext4 și exact două
  discuri pentru RAID1 ext4. RAID5 numai estimare. Teste de overflow/capacitate/dubluri.
- [ ] P1: executor CLI numai în lab explicit, discuri dedicate vdb/vdc, locking,
  identitate verificată din nou înaintea fiecărei operații și confirmare interactivă.
- [ ] P1: plan de recuperare și inspecție read-only pentru ext4/MD degradat; rezultate
  structurate JSON/text și export. Fără promisiuni de rollback sau persistență neimplementate.
- [ ] P2: wizard desktop ușor pentru selecție, capacitate și revizuirea planului;
  Recovery Center cu rezultate reale. Nicio execuție distructivă prin HTTP.
- [ ] P2: matrice fault-injection/mock și teste QEMU opt-in pe QCOW2 noi;
  niciun apply automat în CI. Testarea distructivă necesită o cerere separată explicită.

Auth, autorizare, operații auditate și recovery verificabil rămân condiții înainte
de orice extindere viitoare a HTTP spre modificarea storage-ului.

## Verificare și limite

- [x] Baseline înainte de editare: rustfmt, 15 teste Rust și strict Clippy.
- [x] După editare: rustfmt, 19 teste Rust și Clippy `--all-targets -- -D warnings`.
- [x] Build release `x86_64-unknown-linux-musl` și staging prin `build-core.sh`.
- [x] `bash -n` pentru scripts/tests; `sh -n` pentru scripturile Alpine și OpenRC;
  `node --check web/desktop/app.js`.
- [x] 7 teste fixture Python via `bash tests/check-safety.sh`; fără dispozitive bloc.
- [x] Toate cele 7 teste fixture au trecut și în container Alpine cu BusyBox,
  Python și Bash; cititorul header-ului și generatorul overlay folosesc `/bin/sh`.
- [x] ISO final 0.6.0 construit cu succes; `/init` verificat în imaginea rezultată.
- [x] QEMU implicit și cu CPU Conroe: ordinea branding/OpenRC/DHCP/Zion Core, toate endpointurile,
  assets și refuz POST. Smoke test-ul oprește VM-ul; discul este în mod snapshot.
- [x] Vue scaffold `npm run build` în WSL. Vite avertizează că scriptul clasic
  `/app.js` nu este bundlat. `dist` nu este folosit de staging/ISO. Încercarea Windows
  a eșuat pentru că instalarea locală are executabilele Node din mediul WSL, fără `vue-tsc.cmd`.
- [ ] Browser interactiv, hardware fizic, UEFI, suspend/resume, power-loss și reboot persistence.
- [ ] Provisioning apply, fsck pe un dispozitiv, RAID creation/rebuild și repair: neexecutate.
- [ ] GitHub Actions pe runner remote: neverificat; pașii sursă au fost rulați local.

Build-ul a emis avertismentul upstream `depmod: ... vmlinuz: No such file or directory`
în timpul pregătirii modulelor; generatorul a continuat și a produs ISO-ul. Boot-ul
QEMU este verificarea funcțională necesară, nu ignorarea acestui mesaj.

ISO verificat: `build/iso/zionos-genesis-0.6.0-x86_64.iso` (aproximativ 352 MiB).
SHA256: `06300c5ff7b980865200cc8ab93a41c2eb0327d0b7872562cb425828e6da2791`.
Loguri locale ignorate de Git: `build/audit-iso-final.log`,
`build/audit-qemu-conroe.log`, `build/audit-qemu-default.log`, `build/qemu-smoke.log`.

Fișiere schimbate: `.gitattributes`, `.github/workflows/ci.yml`, `AGENTS.md`, `README.md`,
`core/api/src/{main,storage}.rs`, `platform/alpine/genapkovl-zionos.sh`,
`platform/alpine/zion-lab-provision`, `platform/alpine/initramfs-banner.sh`,
`scripts/{brand-initramfs,build-iso,run-qemu,run-qemu-lab,test-vm}.sh`,
`tests/{check-safety,smoke}.sh`, `tests/test-safety.py`, `web/desktop/README.md`,
acest raport. Eliminat: `scripts/build-iso.sh1`. Artefactele generate nu se publică în Git.

## Surse tehnice pentru interpretarea rezultatelor

- [Documentația oficială smartctl](https://github.com/smartmontools/smartmontools/blob/main/src/smartctl.8.in):
  statusul implicit standby coincide cu device-open failure; `standby,3` le separă.
- [Linux proc documentation](https://www.kernel.org/doc/html/latest/filesystems/proc.html):
  semnificația contoarelor CPU; sursa locală și fixture-urile determină comportamentul API.

Comenzile complete de build/rulare/test sunt în [README](../README.md).
