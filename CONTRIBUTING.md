# Contributing to Eez Funnel

## Code of conduct

By participating, you agree to the Code of Conduct in CODE_OF_CONDUCT.md.

## How to contribute

1. Open an issue first for anything non-trivial.
2. Fork and branch from main. Use feat/<slug>, fix/<slug>, docs/<slug>, chore/<slug>.
3. Keep changes scoped and describe user-visible behavior.
4. Run cargo xtask check before every commit. It must pass.
5. Open a PR with a clear description and verification summary.



## Commit discipline

- One logical change per commit.
- Conventional commit messages: feat:, fix:, docs:, chore:, test:, refactor:.
- Use a clear commit message describing the change.
- Credit contributors where appropriate.

## What we expect

- Tests for every behavior change. Regression test for every bug fix.
- No unwrap, expect, panic!, todo!, unimplemented! on non-test paths.
- No unsafe.
- Discuss new dependencies in the pull request.
- Fuzzing for anything that parses external input.
- Interactive components must be keyboard-navigable and screen-reader friendly.

## Review process

Maintainers review for correctness, test coverage, architecture fit, and style.

## Reporting bugs

Use .github/ISSUE_TEMPLATE/bug_report.md.

## Security

Do not open public issues for vulnerabilities. See SECURITY.md.

## License

By contributing, you agree your contributions are licensed under MIT.
