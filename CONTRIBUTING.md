# Contributing

Development happens on `dev`; `main` is reserved for reviewed releases. Keep
DeepSeek-Desktop in its own repository and never copy this project into the
managed DeepSeek Harness checkout.

Before opening a pull request, run:

```bash
pnpm typecheck
pnpm test
pnpm build
cargo test --manifest-path src-tauri/Cargo.toml
```

Changes that can stop a user's local service or alter files inside the Harness
checkout must include tests and an explicit recovery path in the UI and docs.
