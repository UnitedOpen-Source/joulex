# Spec: Reject Parametrized --setup/--cleanup with --schedule round-robin (Issue #103)

## 1. Problem Statement
In `--schedule round-robin` mode, all setup commands are executed upfront in a batch phase before any timing or warmup runs begin. Cleanups are run at the very end.
When `--setup` or `--cleanup` is parametrized (e.g. `--setup "echo {v} > state"`), each command executes setup with its own parameter value during the upfront batch phase. The last command's setup overwrites the shared state for all earlier benchmarks, causing interleaved runs to execute with the wrong environment state and producing silently invalid results.

## 2. Technical Design
1. **Validation in `Options::validate_against_command_list` (`src/options.rs`):**
   - When `self.schedule == ScheduleMode::RoundRobin` and `!self.allow_setup_with_round_robin`:
     - Check if `self.setup_command` contains any parameter placeholder `{param_name}` from `commands.iter()`.
     - Check if `self.cleanup_command` contains any parameter placeholder `{param_name}` from `commands.iter()`.
     - If either contains a parameter, return an error:
       `The '--setup' and/or '--cleanup' options differ between benchmarks (due to parameter substitution) and cannot be combined with '--schedule round-robin'. Setup runs once per benchmark batch, but round-robin interleaves runs across benchmarks. Use '--prepare' for per-run state, or use the default grouped schedule.`
2. **Override Flag `--allow-setup-with-round-robin`:**
   - Add `--allow-setup-with-round-robin` to allow advanced users to bypass this check if their parametrized setup does not conflict or mutate shared global state.
3. Unparametrized setups and cleanups (identical across all commands) remain allowed.

## 3. Verification Plan
- Integration tests in `tests/integration_tests.rs`:
  - Run round-robin with parametrized setup: assert failure with the descriptive error message.
  - Run round-robin with parametrized setup + `--allow-setup-with-round-robin`: assert success.
  - Run round-robin with unparametrized setup: assert success (existing behavior unchanged).
  - Run grouped schedule with parametrized setup: assert success (existing behavior unchanged).
