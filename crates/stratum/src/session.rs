use std::collections::HashSet;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

/// Allocates unique NONCE_1 values for each connected miner session.
/// NONCE_1 is the server-chosen prefix of the 32-byte block header nonce.
pub struct NonceAllocator {
    nonce1_size: usize,
    counter: AtomicU32,
}

impl NonceAllocator {
    pub fn new(nonce1_size: usize) -> Self {
        assert!(nonce1_size > 0 && nonce1_size < 32, "nonce1_size must be in (0, 32)");
        Self {
            nonce1_size,
            counter: AtomicU32::new(1),
        }
    }

    /// Allocate the next unique NONCE_1 as a hex string.
    pub fn allocate(&self) -> String {
        let val = self.counter.fetch_add(1, Ordering::Relaxed);
        let bytes = val.to_le_bytes();
        // Take only nonce1_size bytes, padding or truncating as needed
        let mut nonce1 = vec![0u8; self.nonce1_size];
        for (i, b) in nonce1.iter_mut().enumerate() {
            if i < bytes.len() {
                *b = bytes[i];
            }
        }
        hex::encode(nonce1)
    }

    pub fn nonce1_size(&self) -> usize {
        self.nonce1_size
    }

    /// Size of NONCE_2 in bytes (what the miner fills in).
    pub fn nonce2_size(&self) -> usize {
        32 - self.nonce1_size
    }
}

/// State for a single connected miner session.
pub struct MinerSession {
    pub session_id: String,
    pub nonce_1: String,
    pub authorized_workers: HashSet<String>,
    pub current_target: Option<String>,
    pub subscribed: bool,
}

impl MinerSession {
    pub fn new(session_id: String, nonce_1: String) -> Self {
        Self {
            session_id,
            nonce_1,
            authorized_workers: HashSet::new(),
            subscribed: false,
            current_target: None,
        }
    }

    pub fn authorize_worker(&mut self, name: &str) -> bool {
        self.authorized_workers.insert(name.to_string())
    }

    pub fn is_worker_authorized(&self, name: &str) -> bool {
        self.authorized_workers.contains(name)
    }
}

/// Generates unique session IDs.
pub fn generate_session_id() -> String {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    let bytes: [u8; 8] = rng.gen();
    hex::encode(bytes)
}

/// Shared pool state accessible by all miner sessions.
pub struct PoolBroadcast {
    pub nonce_allocator: Arc<NonceAllocator>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nonce_allocator_unique() {
        let alloc = NonceAllocator::new(4);
        let a = alloc.allocate();
        let b = alloc.allocate();
        assert_ne!(a, b);
        assert_eq!(a.len(), 8); // 4 bytes = 8 hex chars
    }

    #[test]
    fn nonce2_size_correct() {
        let alloc = NonceAllocator::new(4);
        assert_eq!(alloc.nonce2_size(), 28);
    }

    #[test]
    fn session_authorization() {
        let mut session = MinerSession::new("abc".to_string(), "01020304".to_string());
        assert!(!session.is_worker_authorized("worker1"));
        session.authorize_worker("worker1");
        assert!(session.is_worker_authorized("worker1"));
    }
}
