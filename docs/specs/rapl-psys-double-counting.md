# Spec: Fix RAPL psys Domain Double-Counting (Issue #106)

## 1. Problem Statement
In Linux sysfs (`/sys/class/powercap/intel-rapl`), `intel-rapl:N` domains represent powercap zones.
On modern Intel laptops and desktops, `intel-rapl:0` is typically the CPU package (`package-0`), while `intel-rapl:1` is the platform/system domain (`psys`), which already encompasses CPU package power plus DRAM, system agent, and peripheral chips.
`LinuxRaplSampler::try_new` previously collected all top-level `intel-rapl:N` zones without inspecting their `name` attribute, summing `package-0` + `psys`. This roughly doubled reported energy consumption on laptops and single-socket machines.

On multi-socket servers, `intel-rapl:0` and `intel-rapl:1` correspond to `package-0` and `package-1`, which are disjoint and should both be summed.

## 2. Technical Design
1. **Domain Name Inspection (`name` sysfs attribute):**
   - For each readable top-level domain `intel-rapl:N`, read `path.join("name")`.
2. **Domain Selection Strategy:**
   - Collect all readable candidate domains:
     - Check if any domain has a name starting with `package-` (e.g. `package-0`, `package-1`).
     - If one or more `package-*` domains are present:
       Select **only** the `package-*` domains. Ignore `psys` and other overlapping platform domains.
     - Else if a domain named `psys` is readable:
       Fallback to `psys` alone.
     - Else:
       If neither `package-*` nor `psys` are found, use all readable top-level domains.
3. **Injectable Path for Testing:**
   - Expose `LinuxRaplSampler::try_new_at(base: &Path) -> Option<Self>` alongside `try_new()`.
   - Expose domain selection introspection or test sampling with a mock directory structure using `tempfile`.

## 3. Verification Plan
- Unit tests with mock sysfs directory trees:
  - `{intel-rapl:0 with name "package-0", intel-rapl:1 with name "psys"}` -> only `package-0` is tracked.
  - `{intel-rapl:0 with name "package-0", intel-rapl:1 with name "package-1"}` -> both packages are tracked and summed.
  - `{intel-rapl:0 with name "psys"}` -> `psys` is tracked as fallback.
  - Measure start/stop energy delta and verify calculations.
