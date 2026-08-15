# Build and verify K Framework proofs
# Requires: K Framework 6.0+ (https://github.com/runtimeverification/k)
#
# Run from the proofs/ directory:
#   powershell -File build.ps1

Write-Host "=== Vero Consensus Formal Verification ==="
Write-Host ""

Write-Host "Step 1: Compiling consensus specification..."
kompile consensus-spec.k --backend llvm -o consensus-spec-kompiled
if ($LASTEXITCODE -ne 0) {
    Write-Error "kompile failed"
    exit 1
}
Write-Host "  ✓ consensus-spec.k compiled"

Write-Host "Step 2: Proving safety invariants..."

$claims = @(
    @("thresholdInvariantAfterVote", "P1: Threshold Invariant"),
    @("noBelowThresholdResolution", "P2: No Below-Threshold Resolution"),
    @("monotoneDone", "P3: Monotonic isDone"),
    @("zeroWeightRejected", "P4: Zero Weight Rejected"),
    @("overflowRejected", "P5: Overflow Rejected")
)

$allPassed = $true
foreach ($claim in $claims) {
    $claimName = $claim[0]
    $claimDesc = $claim[1]
    Write-Host "  Proving $claimDesc..."
    
    $output = kprove proofs.k consensus-spec.k --claim $claimName 2>&1
    if ($LASTEXITCODE -ne 0) {
        Write-Warning "  ✗ $claimDesc FAILED"
        Write-Host "    $output"
        $allPassed = $false
    } else {
        Write-Host "  ✓ $claimDesc proved"
    }
}

Write-Host ""
if ($allPassed) {
    Write-Host "=== All invariants verified successfully! ==="
    exit 0
} else {
    Write-Error "=== Some invariants failed verification. ==="
    exit 1
}