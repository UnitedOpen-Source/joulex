# Spec: Natural Layout for Relative Speed Table with --sort=command and --reference (Issue #108)

## 1. Problem Statement
In commit 3d20373 (PR #20), `--sort=command` combined with `--reference` added annotations such as `times faster than <ref>`:
```
Relative speed comparison
        1.00          sleep 0.2
        1.90 ±  0.02 times faster than sleep 0.2  sleep 0.1
        1.97 ±  0.02 times slower than sleep 0.2  sleep 0.4
```
Because the annotation was inserted before the command name, the line read backwards (`1.90 times faster than sleep 0.2 sleep 0.1`).

## 2. Technical Design
1. **Header:**
   - When a reference command is specified, print:
     `Relative speed comparison (reference: <ref_cmd>)`
   - Otherwise, retain `Relative speed comparison`.
2. **Column Layout:**
   - Column 1: Relative speed ratio + stddev (`{:10.2} ± {:5.2}`).
   - Column 2: Command name (`{:<max_cmd_len}`), left-aligned based on the longest command name in the comparison.
   - Column 3: Contextual annotation:
     - For the reference command: `(reference)`
     - For faster commands (`Ordering::Less`): `{ratio} times faster than {ref_cmd}`
     - For slower commands (`Ordering::Greater`): `{ratio} times slower than {ref_cmd}`
     - For identical speed (`Ordering::Equal`): `as fast as {ref_cmd}`
3. When `--reference` is not specified, no annotation column is emitted, preserving existing formatting.

## 3. Verification Plan
- Unit/integration test checking the natural reading order:
  - Ratio -> Command -> Annotation
- Ensure `tests/integration_tests.rs` passes.
- Add an explicit integration test verifying the line structure.
