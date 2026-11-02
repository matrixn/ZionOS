# Genesis 0.6.1 — siguranță, build și QEMU

2026-11-02, continuarea [auditului 0.6](AUDIT_GENESIS_06.md), pe `main`.
Implementare experimentală, fără certificare de securitate. Checklist-ul cumulativ
și comenzile complete sunt în [README](../README.md).

## Remedieri și invariantul păstrat

| Prioritate / risc inițial | Schimbare 0.6.1 | Dovezi și limite |
|---|---|---|
| P1: expunere API pe LAN | Loopback implicit; bind guest numai cu declarație root fw_cfg și vendor QEMU; peer permis 10.0.2.2 sau loopback, Host/Origin validate | Teste router pentru acces local/QEMU, peer extern, DNS rebinding, Origin străin și POST 405; smoke în VM. Fără auth/TLS; hypervisorul și root guest sunt de încredere |
| P1: cursă verificare → mkfs | `--apply` refuzat înainte de apelarea utilitarelor; niciun executor de formatare în 0.6.1 | Test shell cu helper mock; toate căile HTTP rămân read-only. Reactivarea necesită executor verificat, nu doar un lock shell |
| P1: identitate/aliasuri/swap și inventar care permite erori | Un singur guard Rust API/CLI; QCOW2 proaspăt, serial fw_cfg de sesiune și serial VirtIO identic, număr block/sysfs, mountinfo/swap după major:minor, topologie strictă | Fixture-uri vendor/serial/nod/capacitate/ro/removable/partiții/holders/slaves/mount sistem/alias/swap file și erori. Poziții PCI explicite păstrează sistemul vda și laboratorul vdb |
| P1: probe incomplete interpretate ca blank | Descriptor read-only O_EXCL/O_NOFOLLOW, header exact 4096, blkid cu status/stdout/stderr verificate | Code 2 singur nu dovedește absența semnăturilor: poate apărea și la acces refuzat. Fixture-uri reale în fișiere obișnuite, plus planul discului virtual gol în VM. Nu dovedește că întregul disc este gol |
| P2: recovery bazat pe numele unei căi | Guard comun, ext4 confirmat, descriptor exclusiv read-only păstrat către `e2fsck -fn` prin stdin/proc fd, timeout 60s | Mock-ul înlocuiește calea și demonstrează citirea descriptorului original și flagul -fn. VM refuză QCOW2 gol; e2fsck pe ext4 real și fault injection nu au fost executate |
| P2: probe repetitive/blocante | Storage/pools împart cache-ul de 5s; SMART separat 30s; preflight lab 3s. Singleflight, limită globală 128 partiții, bugete fs 2s, semnături 4s, SMART 4s; concurență HTTP maxim 64 | Teste singleflight, expirare, anulare, worker blocat, recuperarea slotului, semnături de disc și partiții eșuate. Syscall-ul blocat nu poate fi anulat: rămâne un worker izolat, fără a porni altele |
| P2: informație UI expirată și formule neverificate | Reset complet lab la eroare, răspuns cu execution_enabled=true refuzat, cereri simultane unite; validare capacitate/dubluri/overflow; polling ascuns/minimizat redus | 3 teste Node pe funcțiile active și DOM minimal. Nu este test de browser complet; Mirror este estimare n-way, viitorul executor RAID1 va cere exact două discuri |
| P2: build lent/variabil | Builder la digest, aports la commit, Cargo pe filesystem WSL nativ, cache Docker cu invalidare după conținut/mod/cheie publică; manifest și publicare ISO atomică după validare | Fixture-uri pentru invalidare și descriptor ISO vechi păstrat; build ISO real. APK-urile rămân mobile, manifestul conține pachetele builderului, nu un SBOM complet live |
| P2: QEMU lent / imagini nepotrivite | KVM auto dacă ioctl CREATE_VM funcționează; TCG fallback; QCOW2 fără backing/external data/dirty/corrupt, override disc/rețea/fw_cfg interzis | Teste header QCOW2/argumente fără VM și smoke real; acces KVM configurat cu autorizarea utilizatorului |

API-ul rulează în continuare ca `nobody:nobody`. Nu au fost adăugate servicii permanente,
dependențe UI externe, endpointuri distructive sau cerințe AVX/SSE4.2. Alpine 3.23,
kernelul LTS, DHCP și runlevel-urile sunt păstrate. Contractul fw_cfg declară proveniența
launcherului; nu autentifică împotriva unui administrator guest sau hypervisor ostil.
`inspection_ready` este parțial și poate fi cached; nu certifică ștergerea sigură.
`execution_enabled` și `eligible_for_lab` rămân false în toate răspunsurile.

## Performanță măsurată local

WSL Ubuntu 22.04, Docker, același laptop și ISO de aproximativ 352 MiB, fără reducerea
colecției de module/firmware. Sunt timpi wall, nu garanții pentru alte sisteme.

| Operație | Timp observat |
|---|---|
| Prima populare a cache-ului ISO și noului builder | 280s |
| Rebuild ISO cu cache, inclusiv verificările core și validarea /init final | 21–25s |
| QEMU KVM: boot și smoke complet, inclusiv lab HTTP | 36s |
| QEMU TCG/Conroe: boot și smoke complet, la reluare | 159s |
| Consola lab KVM: plan real read-only și refuz recovery pe disc gol | 21.9–22.8s |

Utilizatorul WSL `matrixn` a fost adăugat în grupul `kvm` cu autorizare; sesiuni noi
pot citi/scrie `/dev/kvm` și crea o VM. Terminalele vechi necesită redeschidere sau
`newgrp kvm`. Pentru comparație ISA folosește explicit TCG, nu doar `-cpu Conroe` pe KVM.

Builder: Alpine la digest
`sha256:ff7f0c727dd796aa36787bfcd9a64bf35a6ce72a73451b3359fde849b6703d63`,
aports `66c6fed56afee22bdf91a067c8129bfd6b946788`.
Cache-ul/cheia privată de semnare APK sunt în volumul Docker privat per workspace,
nu în repository. Overlay-ul este recalculat la schimbări de core/UI/helper fără
regenerarea inutilă a kernelului. Publicarea prin rename atomic evită rescrierea
ISO-ului deja deschis de QEMU. Nu sunt oferite snapshot-uri APK imuabile sau
reproductibilitate byte-for-byte; builderul privilegiat rămâne o limită de încredere.

## Verificări executate

- [x] 33 teste Rust, rustfmt și Clippy `--all-targets -- -D warnings`.
- [x] 16 teste Python prin `bash tests/check-safety.sh`, folosind fișiere obișnuite/PTY/mock.
- [x] Aceleași 16 fixture-uri trec în container Alpine cu Bash adăugat temporar.
- [x] 3 teste Node și `node --check` pentru desktopul activ.
- [x] Sintaxă Bash pentru scripts/tests și `sh -n` pentru Alpine/OpenRC și scripturile portabile.
- [x] Release musl baseline și staging prin `bash scripts/build-core.sh`.
- [x] ISO final 0.6.1 și extragere/verificare `/init` executabil și banner unic.
- [x] KVM lab HTTP: health, metrics, storage, pools, provisioning, assets, refuz POST,
  identitate validă fără eligibilitate de scriere, ordinea initramfs/OpenRC/DHCP/API,
  loader ready și restaurarea cursorului.
- [x] TCG/Conroe explicit: boot complet, aceleași API/assets/refuz POST și loader ready,
  la reluarea testului după panic-ul primei încercări.
- [x] Consolă QEMU: `--plan` pe QCOW2 nou și refuz `--check` pe disc fără ext4.
- [x] Scaffold Vue: `npm run build`; avertisment Vite pentru app.js clasic nebundlat,
  fără impact asupra staging-ului static folosit de ISO.
- [ ] Hardware fizic, UEFI, power-loss, recovery ext4 real, repair/rollback și RAID.
- [ ] Teste browser interactive ale întregului desktop.

Testele nu au formatat sau reparat dispozitive. Niciun host block-device passthrough.
Refuzul `--apply` este testat cu mock, fără execuție distructivă în VM/CI.
Operațiile legitime păstrate: boot live, DHCP, OpenRC, API neprivilegiat read-only,
desktop static, planul lab și diagnosticul limitat. Suspendarea apply este schimbarea
intenționată de compatibilitate a acestei versiuni.

Artefacte ignorate de Git: `build/iso/zionos-genesis-0.6.1-x86_64.iso` și
`.manifest.json`; loguri `build/061-checks.log`, `061-iso-release.log`,
`061-qemu-lab.log`, `061-qemu-conroe.log`, `061-lab-console*.log`.
SHA256 ISO: `ef28dc6e903cd0d00384cc08a2cc7d5f7b6af133b3bdbc34c11892bc27279be0`.

Prima încercare TCG/Conroe a avut un panic `int3` în `sched_clock_tick`, înaintea
initramfs-ului (`build/061-conroe-panic.log`). Cauza nu a fost determinată; nu am
schimbat kernelul sau dezactivat mecanismele sale de protecție pentru a ascunde eroarea.
Reluarea fără schimbarea ISO/kernelului a trecut în 159s. Aceasta confirmă o pornire
pe ISA emulată Conroe, nu absența problemei intermitente și nici hardware fizic validat.
Testul VM detectează acum explicit kernel panic și nu mai așteaptă inutil timeout-ul.
Verificarea portului folosește SO_REUSEADDR pentru conexiuni închise în TIME_WAIT,
păstrând refuzul unui listener activ, verificat prin fixture.

## Roadmap prioritar

### 0.6.1 — livrat și limite de stabilizare

- [x] P1: închide calea nesigură de scriere, guard comun și refuz sigur la erori.
- [x] P1: API local implicit și acces explicit QEMU, fără mărirea privilegiilor API.
- [x] P2: bugete/cache, stări UI coerente, teste de regresie și build/KVM mai rapide.
- [ ] P2: matrice BIOS/UEFI și browser real; SMART/blkid complet pe hardware cu
  un broker privilegiat limitat, fără a transforma serverul HTTP în root.
- [ ] P2: snapshot APK imuabil, SBOM live și build reproductibil.
- [ ] P2: investigarea panic-ului TCG intermitent; extinderea izolării I/O la citirile
  mici de metadate MD/Btrfs și metrics, care încă sunt sincrone. Bugetele storage nu
  garantează terminarea unui syscall kernel blocat și nici răspuns complet la fiecare cerere.

### 0.7 — condiții înainte de reactivarea execuției

1. **P1:** model Rust tipizat identitate/topologie și plan/validate/simulate; Single ext4
   și exact două discuri RAID1 ext4. Respinge inventar incomplet, sistem, mount, swap,
   holders și semnături necunoscute. Niciun API de scriere.
2. **P1:** executor CLI lab cu descriptor/claim exclusiv păstrat până la terminare,
   identitate revalidată pe descriptor, jurnal și confirmare interactivă. Demonstrează
   refuzul schimbării nodului/serialului și al utilizării concurente, nu doar un lock de proces.
3. **P1:** diagnostic ext4/MD degradat, rezultate JSON/text exportabile și plan de
   recuperare verificabil. Nu promite rollback până când există implementare și teste.
4. **P2:** fault injection pentru timeout, acces refuzat, dispariția discului și lipsă spațiu;
   teste opt-in exclusiv pe QCOW2 dedicate. Orice test distructiv necesită cerere separată.
5. **P2:** wizard ușor pentru revizuirea planului, rezultate recovery reale, browser/UEFI,
   provenance/SBOM și autentificare/autorizare înaintea oricărei extinderi web.

Componente schimbate: `core/api` (access/cache/lab, handlers, storage/pools/provisioning),
`platform/alpine` (launcher contract, utilitare CLI, overlay, branding și builder),
`scripts` (build/cache/publish/QEMU/test), `tests`, CI, politica LF, README și acest raport.
