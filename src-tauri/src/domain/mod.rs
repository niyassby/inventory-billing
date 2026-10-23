//! Pure domain logic shared by services and migrations (no I/O). Mirrors `packages/shared`;
//! both sides are tested against the same vectors in `packages/shared/test-vectors`.

pub mod csv;
pub mod ids;
pub mod money;
pub mod numbering;
pub mod price_lists;
pub mod pricing;
pub mod statement;
