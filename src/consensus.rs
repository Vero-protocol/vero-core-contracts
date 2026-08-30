//! Re-export of the `vero-consensus` crate.
//!
//! The consensus logic has been extracted into its own crate so that the
//! formal verification harnesses in `verification/` can depend on it
//! directly — without pulling in `soroban-sdk` or any other host-dependent
//! code. This shim re-exports all public items so the rest of the contract
//! crate continues to use `crate::consensus::*` paths unchanged.
pub use vero_consensus::{apply_vote, resolution_invariant_holds, ConsensusError, ConsensusState};
