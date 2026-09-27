# Contributing

Install [pre-commit](https://pre-commit.com/) and enable the repository hooks:

```sh
pre-commit install
```

Every commit then runs `cargo fmt --all -- --check` and
`cargo clippy --locked --all-targets -- -D warnings`. Run the same checks before
opening a pull request. The CI workflow also runs formatting, linting, tests,
and builds across supported targets.

Keep each commit focused on one change. Put unrelated formatting or CLI aliases
in separate commits so reviews and regressions remain easy to trace. Open a
pull request for changes to `master` and wait for the required checks and an
independent approval before merging.
