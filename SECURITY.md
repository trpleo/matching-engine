# Security Policy

## Supported Versions

| Version | Supported          |
| ------- | ------------------ |
| 0.1.x   | :white_check_mark: |

## Reporting a Vulnerability

We take the security of Matching Engine seriously. If you discover a security vulnerability, please report it responsibly.

### How to Report

**Please DO NOT report security vulnerabilities through public GitHub issues.**

Instead, please report them via email to:

**security@[your-domain].com** (or contact the maintainer directly)

Alternatively, you can use GitHub's private vulnerability reporting feature:
1. Go to the repository's Security tab
2. Click "Report a vulnerability"
3. Fill out the form with details

### What to Include

Please include the following information in your report:

- Type of vulnerability (e.g., buffer overflow, injection, logic error)
- Full paths of source file(s) related to the vulnerability
- Location of the affected code (tag/branch/commit or direct URL)
- Step-by-step instructions to reproduce the issue
- Proof-of-concept or exploit code (if possible)
- Impact assessment (what an attacker could achieve)

### Response Timeline

- **Initial Response:** Within 48 hours
- **Status Update:** Within 7 days
- **Resolution Target:** Within 90 days (depending on severity)

### What to Expect

1. **Acknowledgment:** We'll confirm receipt of your report
2. **Assessment:** We'll investigate and assess the severity
3. **Updates:** We'll keep you informed of our progress
4. **Resolution:** We'll develop and test a fix
5. **Disclosure:** We'll coordinate disclosure timing with you
6. **Credit:** We'll credit you in the release notes (unless you prefer anonymity)

### Severity Levels

| Level    | Description                                    | Response Time |
|----------|------------------------------------------------|---------------|
| Critical | Remote code execution, data corruption         | 24-48 hours   |
| High     | Denial of service, significant data exposure   | 7 days        |
| Medium   | Limited impact vulnerabilities                 | 30 days       |
| Low      | Minor issues, hardening opportunities          | 90 days       |

## Security Best Practices for Users

When using Matching Engine in production:

1. **Keep updated:** Always use the latest version
2. **Validate inputs:** Sanitize all external inputs before passing to the engine
3. **Monitor:** Implement logging and monitoring for anomalies
4. **Isolate:** Run the engine with minimal privileges
5. **Audit:** Regularly audit your integration code

## Security Measures in Place

- Dependency auditing via `cargo audit` in CI
- Supply chain security via `cargo-deny`
- No unsafe code without explicit justification
- All PRs require security-aware code review

Thank you for helping keep Matching Engine and its users safe!
