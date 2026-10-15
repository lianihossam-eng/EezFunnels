# Contributing to Eez Funnel

## Code of conduct

By participating, you agree to the Code of Conduct in CODE_OF_CONDUCT.md.

## How to contribute

1. Open an issue first for anything non-trivial.
2. Fork and branch from main. Use feat/<slug>, fix/<slug>, docs/<slug>, chore/<slug>.
3. Follow the agent operating contract in AGENTS.md.
4. Run cargo xtask check before every commit. It must pass.
5. Open a PR with a clear description and verification summary.

For project-local agent skills, see `docs/skill-workflow.md` and run
`bash scripts/install-project-skills.sh`. The installer does not configure global tools or hooks.

## Commit discipline

- One logical change per commit.
- Conventional commit messages: feat:, fix:, docs:, chore:, test:, refactor:.
- Reference the task ID at the end of the commit body.
- Sign off with Co-Authored-By when pairing with an agent.

## What we expect

- Tests for every behavior change. Regression test for every bug fix.
- No unwrap, expect, panic!, todo!, unimplemented! on non-test paths.
- No unsafe.
- No new dependencies without an ADR under plan/decisions/.
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
