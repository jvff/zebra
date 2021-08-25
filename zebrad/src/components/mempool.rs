//! Zebra mempool.

mod crawler;

pub use self::crawler::Crawler;

fn is_enabled(_latest_sync_height: &[usize]) -> bool {
    // TODO: Check if synchronizing up to chain tip has finished (#2592).
    true
}
