use rand::Rng;
use std::sync::atomic::{AtomicU16, Ordering};

static COUNTER: AtomicU16 = AtomicU16::new(0);

/// Lexicographically sortable ID: 12 hex chars of millisecond timestamp,
/// 4 of a per-process counter, 8 random. Sorting by ID therefore sorts by
/// creation time, which is what message pagination relies on.
pub fn new_id() -> String {
    let ms = chrono::Utc::now().timestamp_millis().max(0) as u64;
    let seq = COUNTER.fetch_add(1, Ordering::Relaxed);
    let rand: u32 = rand::thread_rng().gen();
    format!("{ms:012x}{seq:04x}{rand:08x}")
}

/// Random token for invite codes, webhook tokens and similar secrets.
pub fn token(len: usize) -> String {
    const ALPHABET: &[u8] = b"abcdefghijkmnopqrstuvwxyzABCDEFGHJKLMNPQRSTUVWXYZ23456789";
    let mut rng = rand::thread_rng();
    (0..len)
        .map(|_| ALPHABET[rng.gen_range(0..ALPHABET.len())] as char)
        .collect()
}
