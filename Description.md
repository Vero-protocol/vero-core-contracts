> **Note — why this file exists:** `Description.md` is the project-description entry consumed by the
> [GrantFox OSS](https://grantfox.io) grant registry for the **FWC26 official campaign listing**.
> The registry pulls this file directly (not `README.md`) to populate the short project blurb shown
> on the grant-program directory page. Keep it to a single, plain-text paragraph — no Markdown
> headings or code blocks — so the registry renderer displays it correctly.
> **Update this file whenever the one-line project pitch changes** (e.g. a major new feature or a
> rename). Do not delete it; if the grant listing moves to a different source, update this note and
> the registry configuration together.

Vero Core Contracts is a Soroban smart contract that brings on-chain GitHub PR verification to Stellar. Trusted guardians cast weighted votes on registered pull requests; once cumulative reputation weight meets a configurable threshold the task is marked complete, creating an immutable, tamper-proof audit trail. An emergency halt system (circuit breaker) allows an admin to instantly freeze all state-changing operations if a vulnerability is discovered.
