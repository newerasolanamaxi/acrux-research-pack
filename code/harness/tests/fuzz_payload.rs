//! Deterministic fuzzing of block payload and envelope decoding (bincode, 1 MiB limit).
//! Property: arbitrary bytes never panic or hang the decoder.
use harness::types::{BlockPayload, Envelope};

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n.max(1) as u64) as usize
    }
}

#[test]
fn payload_decode_never_panics() {
    let mut rng = Rng(0xFEEDFACE);
    // Random garbage of many sizes, including sizes around the 1 MiB cap.
    for round in 0..2_000u64 {
        let len = match round % 5 {
            0 => rng.below(64),
            1 => rng.below(4096),
            2 => (1024 * 1024) - rng.below(4096),
            3 => 1024 * 1024 + rng.below(128),
            _ => rng.below(1024 * 1024),
        };
        let mut bytes = vec![0u8; len];
        for b in bytes.iter_mut().take(len.min(8192)) {
            *b = rng.next() as u8;
        }
        let r1 = std::panic::catch_unwind(|| BlockPayload::decode(&bytes).map(|_| ()));
        assert!(r1.is_ok(), "PANIC decoding {len} random bytes (round {round})");
        let r2 = std::panic::catch_unwind(|| {
            use bincode::Options;
            let _r: Result<Envelope, _> = bincode::DefaultOptions::new()
                .with_fixint_encoding()
                .with_limit(1024 * 1024)
                .reject_trailing_bytes()
                .deserialize(&bytes);
        });
        assert!(r2.is_ok(), "PANIC decoding envelope of {len} random bytes (round {round})");
    }
    // Structured mutations of a valid payload: valid prefix, corrupted tail.
    let payload = BlockPayload { transactions: vec![], fragments: vec![], credits: vec![] };
    let valid = payload.encode(true).unwrap();
    for round in 0..20_000u64 {
        let mut bytes = valid.clone();
        match rng.below(4) {
            0 => bytes.truncate(rng.below(bytes.len() + 1)),
            1 => {
                let i = rng.below(bytes.len());
                bytes[i] ^= 1 << rng.below(8);
            }
            2 => bytes.extend_from_slice(&[rng.next() as u8; 16]),
            _ => {
                // length-prefix games: overwrite the first 8 bytes (vec length, fixint u64)
                for i in 0..8.min(bytes.len()) {
                    bytes[i] = [0xFF, 0x7F, 0x01, 0x00][rng.below(4)];
                }
            }
        }
        let r = std::panic::catch_unwind(|| BlockPayload::decode(&bytes).map(|_| ()));
        assert!(r.is_ok(), "PANIC on structured mutation (round {round})");
    }
    // A claimed-huge vector length with a tiny body: allocation pressure test.
    let mut evil = Vec::new();
    for _ in 0..1000 {
        evil.extend_from_slice(&u64::MAX.to_le_bytes());
    }
    for _ in 0..200 {
        let r = std::panic::catch_unwind(|| BlockPayload::decode(&evil).map(|_| ()));
        assert!(r.is_ok(), "PANIC on huge claimed lengths");
    }
    println!("payload/envelope decode fuzz: no panic, no hang");
              }
