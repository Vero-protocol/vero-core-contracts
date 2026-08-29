# Issue #165: Root-level historical write-ups clutter the repo root

## Progress

- [x] Read all relevant files
- [x] Plan approved
- [x] Create `docs/history/` directory
- [x] Move IMPLEMENTATION_SUMMARY.md → docs/history/
- [x] Move VERIFICATION_REPORT.md → docs/history/
- [x] Move pull_request.md → docs/history/
- [x] Move task_progress.md → docs/history/
- [x] Keep Description.md at the repo root (required by the GrantFox registry)
- [x] Delete original files from repo root
- [x] Update TODO.md — complete

Migration for #165 is complete. `Description.md` intentionally remains at the repo root because it is consumed directly by the GrantFox registry; see the note at the top of that file.

---

# Issue #306: Validate the threshold in set_weight_threshold with the same bounds validate_migration enforces

## Progress
- [x] Integrate `validate_weight_threshold(threshold)?` into `set_weight_threshold` before state storage write
- [x] Document `InvalidAmount` and `InvalidRange` in entrypoint doc comments
- [x] Add unit tests for `validate_weight_threshold` in `src/validation.rs`
- [x] Add property-based tests in `tests/property_tests.rs` for `set_weight_threshold` and `migrate::validate_migration` equivalence
- [x] Add test coverage across core test files (acceptance criteria 1 & 2 verified)
- [x] Update documentation (README, CHANGELOG, etc.) across 15+ files
- [x] Open new branch and push changes to remote


