# Consola de boot ZionOS

Actualizare 2026-11-02, pe `main`. Înlocuiește cele două bannere mici cu un
logo ASCII mare, un panou ANSI și un loader pentru etapele OpenRC reale.

## Ce afișează

- [x] Intro timpuriu în initramfs, folosind numai builtins shell.
- [x] Logo ASCII ZIONOS, slogan, culori cyan/albastru/magenta și puls de lumină.
- [x] Indicator animat de activitate, la cel mult 4 cadre/secundă, fără procent fictiv.
- [x] Logo fix: animația actualizează numai bara, iar mesajele se citesc o dată pe secundă.
- [x] Milestone-uri: live root, hardware/sysinit, identitate/rețea/boot, Zion Core/default.
- [x] Ultimele patru linii reale de diagnostic, limitate la lățimea panoului.
- [x] Ecran final „online” numai după răspunsul health de la Zion Core.
- [x] Stare explicită „API pending” dacă OpenRC a terminat, dar API-ul nu răspunde încă.
- [x] Cursor restaurat și rânduri libere pentru promptul de login.

Intro-ul din initramfs este static; animația începe în wrapper-ul OpenRC și acoperă
inclusiv așteptarea montării/verificării modloop. Nu adăugăm întârzieri pentru efecte
vizuale și nu pretindem să măsurăm procentul de încărcare al kernelului sau serviciilor.

## Implementare și comportament la eroare

`platform/alpine/boot-ui.sh` este rendererul comun, inclus atât în `/init`, cât și
în overlay la `/usr/local/lib/zionos/boot-ui.sh`. Corpul init-ului Alpine rămâne intact.
`platform/alpine/zion-init-wrapper` apelează `/sbin/openrc` pentru fiecare etapă
`sysinit`, `boot`, `default`, în aceeași ordine BusyBox init. Shutdown rămâne apelul
standard OpenRC. Nu schimbăm DHCP, kernelul sau privilegiile API-ului.

OpenRC rulează în foreground cu stdin-ul păstrat. Numai rendererul este un proces
temporar de fundal, oprit și colectat la sfârșitul fiecărei etape. Logurile stdout
și stderr sunt păstrate integral în filesystemul live RAM, cu permisiuni restrictive:

```sh
cat /var/log/zion-boot/sysinit.log
cat /var/log/zion-boot/boot.log
cat /var/log/zion-boot/default.log
```

Logurile sunt redate și în scrollback-ul serial după fiecare etapă. Dacă OpenRC
returnează o eroare, wrapper-ul afișează diagnosticul complet și codul de eroare;
codul este propagat către init. Animația nu transformă o eroare într-un rezultat OK.
Nu există jurnal persistent: logurile din RAM se pierd la reboot.
Crearea logurilor folosește un umask restrictiv numai într-un subshell; serviciile
OpenRC păstrează umask-ul moștenit. Un fixture verifică explicit această proprietate.

Panoul folosește o regiune de scroll numai pentru diagnostic, restaurată la final.
Un prim test Conroe cu redesenare completă la fiecare cadru a depășit timeout-ul
de boot. Implementarea a fost optimizată să păstreze logo-ul și să actualizeze numai
regiunile dinamice; testele verifică și absența redesenărilor repetate ale antetului.

## Mod clasic și limitări

- Adaugă `zion.splash=off` la linia kernelului în bootloader pentru logurile OpenRC clasice.
- Fără TTY, cu `TERM=dumb` sau la dimensiuni detectate sub 74 coloane/24 rânduri,
  wrapper-ul execută OpenRC direct. Lipsa rendererului sau imposibilitatea creării
  logului păstrează aceeași cale de fallback.
- Pentru aspectul complet, folosește un terminal de minimum 80×24 cu ANSI.
- Kernelul și bootloader-ul pot afișa mesaje înainte de intro. Mesajele kernelului
  emise direct pe consolă pot intersecta temporar panoul; următorul cadru îl redesenează.
- Pe serial fără dimensiuni raportate, rendererul presupune consola standard 80×24.
  Redimensionarea ferestrei host nu este comunicată automat prin UART.
- Ecranul final confirmă API-ul local, nu livrarea de servicii publice, autentificare,
  sănătatea discurilor sau eligibilitatea pentru provisioning.

## Build și verificare

```bash
cd /mnt/c/Users/radut/Workspace/zionos
bash tests/check-safety.sh
bash scripts/build-iso.sh
bash scripts/test-vm.sh -cpu Conroe
bash scripts/run-qemu.sh -display none
```

Cu VM-ul interactiv pornit, deschide [http://localhost:8080](http://localhost:8080).
Din alt terminal rulează `bash tests/smoke.sh`. Oprește VM-ul înainte de alt test;
în consola serială: `Ctrl+A`, apoi `X`.

- [x] 13 teste fixture în WSL și Alpine/BusyBox: animație în PTY, loguri stdout/stderr, status de eroare,
  restaurare cursor/scroll după SIGTERM, ready/pending, verbose, pipe, terminal mic,
  TERM=dumb, overlay și initramfs.
- [x] 19 teste Rust, rustfmt/Clippy strict și verificări de sintaxă shell/JavaScript.
- [x] ISO reconstruit și `/init` verificat în imaginea finală.
- [x] Boot QEMU implicit și `Conroe`: animație, ordine initramfs/OpenRC, DHCP,
  toate endpointurile/assets, refuz POST, finalizare ready și restaurarea cursorului.
- [ ] BIOS/UEFI pe hardware fizic și toate tipurile de terminal: neexecutate.

Nu există operații noi pe discuri sau endpointuri HTTP noi în această modificare.

ISO generat: `build/iso/zionos-genesis-0.6.0-x86_64.iso`.
SHA256: `9fe23c2ff6fa09a6118247bdebdde6d5bd6bb59c0b414fdcc456585b15dd093c`.
Loguri locale ignorate de Git: `build/loader-release-iso.log`,
`build/loader-release-conroe.log`, `build/loader-release-default.log` și `build/qemu-smoke.log`.

Fișiere modificate/adăugate: `.gitattributes`, `.github/workflows/ci.yml`,
`README.md`, `docs/AUDIT_GENESIS_06.md`, acest document,
`platform/alpine/{boot-ui.sh,zion-init-wrapper,initramfs-banner.sh,genapkovl-zionos.sh}`,
`scripts/{brand-initramfs.sh,build-iso.sh,test-vm.sh}` și `tests/test-safety.py`.
