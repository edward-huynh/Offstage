# Contributing

1. Fork, branch from `main`, keep PRs focused.
2. Run `npm run check` before pushing; CI runs the same steps.
3. Rust: `cargo fmt`, no clippy warnings. TS: strict mode, no unused code.
4. Never commit keys, `.env` files or model weights. CI runs gitleaks on every push.
5. For a new LLM provider, prefer adding a preset in `crates/llm/src/presets.rs` over new code.

Commit messages follow Conventional Commits (`feat:`, `fix:`, `docs:`, `chore:`).
