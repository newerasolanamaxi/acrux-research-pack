//! Measures ThresholdCache tombstone (decoded / expired) growth and its serialization cost.
//! Local measurement only: no node, no network.
use acrux_execution::{crypto_helpers, threshold};
use ed25519_dalek::SigningKey;
use solana_sdk::{
    compute_budget::ComputeBudgetInstruction,
    hash::Hash,
    signature::{Keypair, Signer},
    system_instruction,
    transaction::Transaction,
};

fn seal_unique(payer: &Keypair, signer: &SigningKey) -> (String, Vec<threshold::FragmentSubmission>) {
    let tx = Transaction::new_signed_with_payer(
        &[
            ComputeBudgetInstruction::set_compute_unit_limit(20000),
            system_instruction::transfer(&payer.pubkey(), &Keypair::new().pubkey(), 1000),
        ],
        Some(&payer.pubkey()),
        &[payer],
        Hash::new_unique(),
    );
    let bytes = bincode::serialize(&tx).unwrap();
    let header = acrux_execution::execution::TxHeader {
        id: hex::encode(crypto_helpers::hash(&tx.message.serialize())),
        fee_payer: payer.pubkey().to_string(),
        priority_fee_per_cu: "0".into(),
        compute_limit: 20000,
        writable_accounts: vec![payer.pubkey().to_string()],
        target_slot: 8,
    };
    let id = header.id.clone();
    let sealed = threshold::seal_for_receivers(&bytes, header, signer, 3, 2, &[1, 2, 3]).unwrap();
    (id, sealed)
}

fn sizes(cache: &threshold::ThresholdCache) -> (usize, usize) {
    let json = serde_json::to_vec(cache).unwrap();
    let bin = bincode::serialize(cache).unwrap();
    (json.len(), bin.len())
}

#[test]
fn tombstone_growth_is_unbounded_and_serialized() {
    let payer = Keypair::new();
    let signer = SigningKey::from_bytes(&payer.to_bytes()[..32].try_into().unwrap());
    let expiry = 8u64;

    // Path 1: one included fragment per unique id, then expiry -> permanent `expired` tombstone.
    let mut cache = threshold::ThresholdCache::new(expiry).unwrap();
    let mut report = vec![];
    for batch in 0..5 {
        for _ in 0..500 {
            let (_id, sealed) = seal_unique(&payer, &signer);
            cache
                .accept(8, sealed[0].receiver, sealed[0].verify().unwrap())
                .unwrap();
        }
        let expired = cache.expire(8 + expiry + 1);
        assert_eq!(expired.len(), 500 * (batch + 1) - 500 * batch);
        report.push(((batch + 1) * 500, sizes(&cache)));
    }
    let (j0, b0) = sizes(&threshold::ThresholdCache::new(expiry).unwrap());
    println!("empty cache: json={j0}B bincode={b0}B");
    let mut last = (j0, b0);
    for (n, (j, b)) in &report {
        println!("expired tombstones n={n}: json={j}B bincode={b}B (+{}B json/entry, +{}B bin/entry)",
            (j - last.0) / 500, (b - last.1) / 500);
        last = (*j, *b);
    }
    // Checkpoint amplification: the node clones the whole cache into every cblock's checkpoint
    // (threshold_checkpoints, retained for ~8+ cblocks) and into state.redb's Meta, and serves
    // it whole in ExecutionSnapshot. Simulate the retained clones.
    let clones: Vec<_> = (0..11).map(|_| cache.clone()).collect();
    let heap: usize = clones.iter().map(|c| serde_json::to_vec(c).unwrap().len()).sum();
    println!("11 retained checkpoint clones serialize to {heap}B total");

    // Path 2: fully decoded ids stay in `decoded` forever too.
    let mut cache2 = threshold::ThresholdCache::new(expiry).unwrap();
    for _ in 0..500 {
        let (_id, sealed) = seal_unique(&payer, &signer);
        for i in [0usize, 1] {
            cache2
                .accept(8, sealed[i].receiver, sealed[i].verify().unwrap())
                .unwrap();
        }
    }
    let (j2, b2) = sizes(&cache2);
    println!("decoded tombstones n=500: json={j2}B bincode={b2}B (+{}B json/entry)", (j2 - j0) / 500);

    // The point of the measurement: nothing ever shrinks these sets.
    let before = sizes(&cache);
    cache.expire(u64::MAX / 2);
    let after = sizes(&cache);
    assert_eq!(before, after, "expire() must not shrink the tombstone sets");
    println!("tombstones survive expire(u64::MAX/2): json {}B -> {}B", before.0, after.0);
      }
