---
name: Feature Request
about: Propose a new capability for vero-core-engine
title: "[FEATURE] <concise title>"
labels: ["feature-request", "needs-triage"]
assignees: []
---

## Description
<!-- What should the engine do that it cannot do today? One paragraph maximum. -->

## Problem Statement
<!-- What workflow, integration, or user need is blocked without this feature? -->

## Requirements
<!-- Numbered, testable requirements. Each "MUST" / "SHOULD" statement maps 1:1 to an acceptance criterion below. -->

1. MUST …
2. SHOULD …
3. MAY …

## Proposed Implementation
<!-- Optional: sketch the design — module, interface changes, data flow. Delete if you have no preference. -->

```
engine-core / engine-bridge — affected files:
- src/...
```

## Acceptance Criteria
<!-- Checkbox list. Each item must be objectively verifiable. -->

- [ ] AC-1: …
- [ ] AC-2: …
- [ ] AC-3: Unit/integration tests pass with ≥ 90% coverage on new code.
- [ ] AC-4: `BUILD_ENGINE.sh health-check` exits 0 with the feature enabled.

## Security Considerations
<!-- Does this feature touch auth, cryptography, governance, or funds? If yes, explain the threat model. -->

- **Auth changes**: none / describe
- **Cryptographic surface**: none / describe
- **Fund flow impact**: none / describe
- **Threat vectors considered**: none / list

## Definition of Done
- [ ] Feature branch merged to `main` with all AC items checked.
- [ ] DEVELOPMENT_ROADMAP.md milestone updated.
- [ ] `docs/` updated if public interface or architecture changes.
- [ ] No new `cargo clippy` warnings; `npm run lint` clean.
- [ ] Changelog entry added.
