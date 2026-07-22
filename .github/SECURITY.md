# Security Policy

We take the security of the CAVS Hub CLI seriously. Thank you for helping keep
it and its users safe.

## Reporting a vulnerability

**Please do not report security vulnerabilities through public GitHub issues,
discussions, or pull requests.**

Instead, use one of the following private channels:

1. **GitHub private vulnerability reporting** (preferred): open a report from
   the repository's **Security → Report a vulnerability** tab.
2. **Email**: send details to **orelvis15@gmail.com** with the subject
   `SECURITY: cavs-hub-cli`.

Please include as much of the following as you can:

- a description of the issue and its impact;
- the version / commit affected (`cav --version`);
- clear reproduction steps or a proof of concept;
- any suggested remediation.

We will acknowledge your report, keep you updated on our progress, and credit
you once a fix is released (unless you prefer to remain anonymous).

## Scope

This policy covers the `cav` CLI in this repository. Issues in the CAVS core
belong in [`cavs-oss`](https://github.com/orelvis15/cavs-oss); issues in the
control-plane API belong in the CAVS Hub service repository.

### Dependency advisories

A CVE appearing in a dependency does not by itself mean `cav` is exploitable.
When reporting a dependency advisory, please include evidence that the
vulnerable code path is reachable from `cav` (a call chain or a proof of
concept). Reports without a demonstrated impact may be closed.

## Supported versions

Only the latest released version receives security fixes while the project is
pre-1.0.
