# Spec: `--check-system[=warn|strict]` (#64)

## Problem
Most noisy or irreproducible results come from the environment: the `powersave`
governor, turbo boost, other load, running on battery, thermal throttling. Energy
measurements are even more sensitive. Users have to know all of this themselves.

## Contract
```
--check-system           report problems before benchmarking, then continue
--check-system=strict    abort with exit code 4 if any check does not pass
```
Report (stdout, before the first benchmark; suppressed by `--style none`, except
that a strict failure still prints it to stderr):
```
System check:
  ✖ CPU governor     powersave on 1 CPU  → sudo cpupower frequency-set -g performance
  ✔ Load average     0.10 (10 CPUs)
```
Status: ✔ ok, ! warn (a condition: load, battery, heat, Low Power Mode),
✖ fail (a setting known to add noise: governor, turbo boost). Strict fails on
anything that is not ✔.

| Check | Linux | macOS |
|---|---|---|
| CPU governor | all `cpu*/cpufreq/scaling_governor` = performance | — |
| Turbo boost | `intel_pstate/no_turbo` = 1 or `cpufreq/boost` = 0 | — |
| Load average | 1-min load < max(1, 0.1 × CPUs) (`/proc/loadavg`) | same (`getloadavg`) |
| Power source | a `Mains` supply online (skipped without one, e.g. servers) | `pmset -g batt` |
| Low Power Mode | — | `pmset -g` `lowpowermode 0` |
| Thermal | max `thermal_zone*/temp` < 85 °C | `pmset -g therm` `CPU_Speed_Limit` = 100 |

Windows: no checks yet; the report says so.

## Environment metadata (always, not only with `--check-system`)
`joulex.system` in the JSON gains `cpu_model` (Linux `/proc/cpuinfo`, macOS
`sysctlbyname("machdep.cpu.brand_string")`, Windows `PROCESSOR_IDENTIFIER`) and
`kernel` (`uname` release on Unix). Both are omitted when unknown. There is still
no hostname or user name.

## Testability
The Linux checks read below a root directory. The hidden environment variable
`JOULEX_SYSTEM_CHECK_ROOT` points it at a fake `/sys` + `/proc` tree on any
platform, which the integration tests use. It only redirects reads.

## Tests
- Unit: governor summary, turbo (Intel/AMD), loadavg parsing and threshold, power
  supplies, thermal, `pmset` parsing (batt, lowpowermode, CPU_Speed_Limit), and a
  full fake root.
- Integration (`tests/check_system_tests.rs`): report + hints and continue; strict →
  exit 4 without benchmarking; strict passes on a quiet system; strict with
  `--style none` still explains; no report without the option; JSON `system` fields.

## Also in this change
`tests/diagnostics_tests.rs` ramp test hardened like the multimodal one (`-N`,
20 ms + 3 ms/run): it failed once in a full-suite run on a loaded laptop.
