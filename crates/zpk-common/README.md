# zpk-common

Shared ELF architecture, musl interpreter, DT_NEEDED/RUNPATH and GNU ISA-needed-note
inspection used by Forge and Runtime. This split is justified by two real consumers;
Forge keeps its `elf` re-export for compatibility. This crate has no builder, process
execution, Docker or init-system integration. Validation is exercised by both consumers'
regression tests. It is not a complete instruction decoder or runtime dependency solver.
