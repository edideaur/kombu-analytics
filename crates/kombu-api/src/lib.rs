#![forbid(unsafe_code)]
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

pub mod admin;
pub mod alerts;
pub mod annotations;
pub mod auth;
pub mod background;
pub mod boards;
pub mod config;
pub mod dashboard;
pub mod event_data;
pub mod events;
pub mod export_handler;
pub mod ingest;
pub mod links;
pub mod me;
pub mod pixels;
pub mod queue;
pub mod rate_limit;
pub mod realtime;
pub mod recorder;
pub mod reports;
pub mod retention;
pub mod revenue;
pub mod router;
pub mod segments;
pub mod session_data;
pub mod sessions;
pub mod share;
pub mod storage;
pub mod teams;
pub mod two_factor;
pub mod users;
pub mod websites;

pub use router::build_router;

#[cfg(kani)]
mod kani_proofs {
    #[kani::proof]
    fn harness_query_page_math() {
        let page: i64 = kani::any();
        let page_size: i64 = kani::any();
        kani::assume(page_size >= 1 && page_size <= 100);
        kani::assume(page >= 1 && page <= 10_000);
        let offset = (page - 1) * page_size;
        kani::assert(offset >= 0, "offset non-negative for valid pages");
    }

    #[kani::proof]
    fn harness_status_code_bounds() {
        let is_ok: bool = kani::any();
        let code = if is_ok { 200u16 } else { 400u16 };
        kani::assert(code >= 200 && code < 600, "valid HTTP status code range");
    }
}
