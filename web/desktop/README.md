# ZionOS Desktop

- [x] Desktopul din ISO folosește `index.html` + `app.js`, fără framework la runtime.
- [x] `scripts/build-core.sh` copiază aceste două fișiere în staging.
- [x] Teme, dock, ferestre, monitorizare, Storage Manager și Capacity Lab în shell-ul static.
- [ ] `src/`, Vue, TypeScript și Vite sunt un scaffold separat, neintegrat în ISO.
- [ ] Teste automate DOM/browser pentru ferestre, erori API și simulările de capacitate.

Verifică desktopul activ cu `node --check web/desktop/app.js` și, cu VM-ul pornit,
`bash tests/smoke.sh`. `npm run build` verifică scaffold-ul, nu integrarea Vue în ISO.

Vezi [README-ul proiectului](../../README.md) și [auditul](../../docs/AUDIT_GENESIS_06.md)
pentru build, rulare, riscuri și rezultatele verificărilor.
