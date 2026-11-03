# ZionOS Desktop

Genesis 0.7 adds the first-party [authenticated Package Center](../../docs/GENESIS_07.md)
in `apps/package-center.js`/`.css`, integrated with the existing dock/window manager.
It uses real Core/broker operations and production persistent-volume startup, not
browser-local package state. Authentication is confined to the package control plane;
NAS storage APIs stay read-only. [Architecture](../../docs/GENESIS_07_ARCHITECTURE.md).

TASK 05 adds [three-boot native integration](../../docs/zpk/E2E.md), using the same
window manager and Core asset routes. Its Chromium observer keeps a real app window
open during local guest CLI upgrade/uninstall and checks live revocation. This does
not grant privileged lifecycle/settings operations to browser JavaScript.

- [x] ISO: `index.html`, `app.js`, `apps/registry.js`, `apps/bridge.js`, fără framework la runtime.
- [x] `scripts/build-core.sh` copiază aceste fișiere în staging.
- [x] Teme, dock, ferestre, monitorizare, Storage Manager și Capacity Lab în shell-ul static.
- [ ] `src/`, Vue, TypeScript și Vite sunt un scaffold separat, neintegrat în ISO.
- [x] 3 teste Node pentru capacitate, resetarea stării la erori lab și coalescarea cererilor.
- [x] Genesis 0.6.1: apply indisponibil, readiness parțial separat de autorizarea scrierilor;
  polling oprit în tab ascuns și storage minimizat.
- [x] ZPK dinamic: launcher/search, icon, instanțe multiple, dock/pin și tastatură.
- [x] SDK cu iframe opac, CSP, verificări sursă/sesiune/permisiuni și teme/dialoguri/notificări.
- [x] Chromium real desktop/mobil și QEMU nativ: ferestre, SDK, disable/uninstall și revocare assets.
- [x] Package Center: catalog, upload/preview/review, lifecycle,
  upgrade/rollback/uninstall, progres real, teme dark/light și layout mobil.
- [x] Un singur setup/login/logout ZionOS în `session.js`/`.css`; Package Center
  folosește sesiunea globală, fără formulare sau conturi separate. Conturile vechi
  păstrează parola și primesc username `admin`. Roluri standard/multi-user: viitor.
- [x] Carduri apăsabile în grid, navigare din tastatură, pagină cu descriere,
  compatibilitate/dependențe/permisiuni și acțiuni principale sus. Install duce
  separat la aprobarea permisiunilor; navigarea prin catalog nu rezervă preview-uri.
- [ ] Settings/fișiere SDK și origini dedicate aplicațiilor; brokerul de pachete
  nu acordă aceste capabilități cadrelor aplicațiilor.

`bash tests/apps/check.sh` verifică SDK-ul generat, TypeScript și 17 teste Node
(14 SDK/registry/bridge + 3 desktop existente). `--browser` folosește un Core debug
neprivilegiat și servicii mock; `--vm` verifică suplimentar backendul nativ OpenRC.
Vezi [App SDK](../../docs/zpk/APP_SDK.md) pentru setup și limitele de securitate.
`bash tests/package-center/check.sh --browser` verifică separat grid-ul cu șapte
pachete fixture, navigarea, acțiunile după stare și layout-ul mobil; fără servicii
native sau installer executat pe host. `--vm` verifică operațiile reale în QEMU.
Shift-launch sau `+` deschide altă instanță când manifestul permite. Pin persistă în
browserul shell-ului, separat de settings API indisponibil pentru aplicații.
Navigarea/reîncărcarea întregului document iframe închide fereastra. CSP nu garantează
blocarea tuturor cererilor de navigare sau izolarea CPU/RAM. Bridge-ul nu este privilegiat.

Verifică desktopul activ cu `node --check web/desktop/app.js` și, cu VM-ul pornit,
`bash tests/smoke.sh`; rulează `node --test tests/desktop.test.cjs` pentru fixture-urile
funcțiilor active. `npm run build` verifică scaffold-ul, nu integrarea Vue în ISO.

Vezi [README-ul proiectului](../../README.md) și [auditul](../../docs/AUDIT_GENESIS_06.md)
pentru build, rulare, riscuri și rezultatele verificărilor.
