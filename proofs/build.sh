#!/usr/bin/env bash
# Build and verify K Framework proofs
# Requires: K Framework 6.0+ (https://github.com/runtimeverification/k)
#
# Run from the proofs/ directory:
#   bash build.sh

set -euo pipefail

echo "=== Vero Consensus Formal Verification ==="
echo ""

echo "Step 1: Compiling consensus specification..."
kompile consensus-spec.k --backend llvm -o consensus-spec-kompiled
echo "  ✓ consensus-spec.k compiled"

echo "Step 2: Proving safety invariants..."

CLAIMS=(
    "thresholdInvariantAfterVote:P1: Threshold Invariant"
    "noBelowThresholdResolution:P2: No Below-Threshold Resolution"
    "monotoneDone:P3: Monotonic isDone"
    "zeroWeightRejected:P4: Zero Weight Rejected"
    "overflowRejected:P5: Overflow Rejected"
)

ALL_PASSED=true
for claim in "${CLAIMS[@]}"; do
    CLAIM_NAME="${claim%%:*}"
    CLAIM_DESC="${claim##*:}"
    echo "  Proving ${CLAIM_DESC}..."
    
    if kprove proofs.k consensus-spec.k --claim "${CLAIM_NAME}"; then
        echo "  ✓ ${CLAIM_DESC} proved"
    else
        echo "  ✗ ${CLAIM_DESC} FAILED"
        ALL_PASSED=false
    fi
done

echo ""
if [ "$ALL_PASSED" = true ]; then
    echo "=== All invariants verified successfully! ==="
else
    echo "=== Some invariants failed verification. ==="
    exit 1
fi