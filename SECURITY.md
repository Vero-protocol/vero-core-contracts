# Security Policy

Vero Core Contracts handles on-chain consensus, escrow, and reward integrity on Soroban. We take security reports seriously and coordinate disclosure privately with reporters.

## Supported Versions

Security fixes are applied to the latest release and to the `main` branch, which is the actively developed line.

| Version | Supported |
| ------- | --------- |
| `main` (unreleased) | ✅ |
| `0.1.0` (latest release) | ✅ |
| `0.1.0-feat.69` | ❌ |
| Older versions | ❌ |

If you are running an unsupported version, upgrade to the latest release (or `main`) before reporting; fixes are only backported where explicitly agreed with the maintainers.

## Reporting a Vulnerability

**Please do not open a public issue for a security vulnerability.** Instead, report it privately so it can be fixed before details are disclosed.

### Preferred: GitHub Private Vulnerability Reporting

1. Go to the repository's **Security** tab: `https://github.com/Vero-protocol/vero-core-contracts/security`
2. Click **Report a vulnerability** and fill in the form.
3. You will receive an acknowledgment, and the report stays private until it is triaged and fixed.

### What to include

To help us triage quickly, please include:

- **Summary** — what the vulnerability is and its impact on the protocol (fund loss, consensus manipulation, denial of service, etc.)
- **Affected surface** — contract entry point(s) / module(s) and, if known, the affected `ContractError` variant or storage key
- **Severity estimate** — e.g. critical (direct fund loss), high (state corruption / consensus bypass), medium (griefing / DoS), low (informational)
- **Reproduction steps** — a minimal test or call sequence that triggers the issue, plus the expected vs. actual behavior
- **Suggested fix** (optional) — any mitigation or patch you have in mind

### What happens next

- **Acknowledgment** within **3 business days** of submission.
- **Status update** within **7 business days** — triage result and planned remediation (e.g. a patched WASM deployed via the multi-sig upgrade path, followed by a pause/circuit-breaker if the issue is critical).
- **Disclosure** — after a fix is released, we coordinate public disclosure with you and credit reporters who want recognition.

## Security Best Practices for Deployers

- Only grant roles (`GuardianManager`, `TaskManager`, `ConfigManager`, `EmergencyManager`, `TreasuryManager`) to addresses that need them, and use a multi-sig for the admin account.
- Guardians must lock tokens above the configured `LockThreshold` before voting; verify thresholds match your risk model.
- In an incident, use the manual `pause` (requires `EmergencyManager`) or the corroborated circuit breaker, then remediate via the multi-sig upgrade path.
- Report suspected abuse of the failure-reporting channel (`record_failure`) rather than pausing silently — the breaker is designed to be spam-resistant.
