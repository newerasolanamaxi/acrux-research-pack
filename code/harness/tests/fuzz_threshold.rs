//! Deterministic mutation fuzzing of threshold fragment parsing/validation and cache sequences.
//! Property: untrusted input never panics; invalid input is rejected; cache invariants hold.
use acrux_execution::threshold;
use ed25519_dalek::SigningKey;
use solana_sdk::{
    compute_budget::ComputeBudgetInstruction,
    hash::Hash,
    signature::{Keypair, Signer},
    system_instruction,
    transaction::Transaction,
};

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

fn valid_seal() -> Vec<threshold::FragmentSubmission> {
    let payer = Keypair::new();
    let tx = Transaction::new_signed_with_payer(
        &[
            ComputeBudgetInstruction::set_compute_unit_limit(20000),
            system_instruction::transfer(&payer.pubkey(), &Keypair::new().pubkey(), 1000),
        ],
        Some(&payer.pubkey()),
        &[&payer],
        Hash::new_unique(),
    );
    let bytes = bincode::serialize(&tx).unwrap();
    let header = acrux_execution::execution::TxHeader {
        id: hex::encode(acrux_crypto::hash(&tx.message.serialize())),
        fee_payer: payer.pubkey().to_string(),
        priority_fee_per_cu: "0".into(),
        compute_limit: 20000,
        writable_accounts: vec![payer.pubkey().to_string()],
        target_slot: 8,
    };
    let signer = SigningKey::from_bytes(&payer.to_bytes()[..32].try_into().unwrap());
    threshold::seal_for_receivers(&bytes, header, &signer, 3, 2, &[1, 2, 3]).unwrap()
}

const B64CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/=!@#$%^&*";

fn mutate_string(rng: &mut Rng, s: &str, max: usize) -> String {
    let mut bytes = s.as_bytes().to_vec();
    match rng.below(6) {
        0 => {
            // random truncation
            bytes.truncate(rng.below(bytes.len() + 1));
        }
        1 => {
            // flip bytes
            for _ in 0..=rng.below(8) {
                if bytes.is_empty() {
                    break;
                }
                let i = rng.below(bytes.len());
                bytes[i] = B64CHARS[rng.below(B64CHARS.len())];
            }
        }
        2 => {
            // extend near the cap
            while bytes.len() < max.min(bytes.len() + rng.below(max)) {
                bytes.push(B64CHARS[rng.below(B64CHARS.len())]);
            }
        }
        3 => bytes.clear(),
        4 => {
            // unicode junk
            bytes = "💣🔥\u{0}\u{ffff}".repeat(rng.below(16) + 1).into_bytes();
        }
        _ => {
            // splice two halves
            let i = rng.below(bytes.len() + 1);
            bytes.splice(i..i, B64CHARS[..rng.below(B64CHARS.len())].iter().copied());
        }
    }
    String::from_utf8_lossy(&bytes).into_owned()
}

#[test]
fn fragment_parse_never_panics_on_mutated_input() {
    let sealed = valid_seal();
    let mut rng = Rng(0xA0CFF1E3);
    let mut accepted = 0usize;
    for round in 0..20_000u64 {
        let mut value = serde_json::to_value(&sealed[rng.below(3)]).unwrap();
        let obj = value.as_object_mut().unwrap();
        match rng.below(10) {
            0 => {
                let f = mutate_string(&mut rng, obj["fragment"].as_str().unwrap(), 2100);
                obj.insert("fragment".into(), f.into());
            }
            1 => {
                let k = mutate_string(&mut rng, obj["keyShare"].as_str().unwrap(), 60);
                obj.insert("keyShare".into(), k.into());
            }
            2 => {
                let s = mutate_string(&mut rng, obj["headerSignature"].as_str().unwrap(), 100);
                obj.insert("headerSignature".into(), s.into());
            }
            3 => {
                obj.insert("n".into(), (rng.next() % 300).into());
            }
            4 => {
                obj.insert("t".into(), (rng.next() % 300).into());
            }
            5 => {
                obj.insert("index".into(), (rng.next() % 300).into());
            }
            6 => {
                obj.insert("receiver".into(), (rng.next() % 300).into());
            }
            7 => {
                let header = obj.get_mut("header").unwrap().as_object_mut().unwrap();
                let key = ["id", "feePayer", "priorityFeePerCu", "targetSlot", "computeLimit"][rng.below(5)];
                match key {
                    "id" => {
                        let id = mutate_string(&mut rng, header["id"].as_str().unwrap(), 80);
                        header.insert("id".into(), id.into());
                    }
                    "priorityFeePerCu" => {
                        let p = mutate_string(&mut rng, header["priorityFeePerCu"].as_str().unwrap(), 40);
                        header.insert("priorityFeePerCu".into(), p.into());
                    }
                    "feePayer" => {
                        let f = mutate_string(&mut rng, header["feePayer"].as_str().unwrap(), 60);
                        header.insert("feePayer".into(), f.into());
                    }
                    _ => {
                        header.insert(key.into(), (rng.next()).into());
                    }
                }
            }
            8 => {
                // replace the whole value with random JSON
                value = serde_json::json!({"header": rng.next(), "n": rng.next() % 5, "garbage": vec![rng.next(); rng.below(4)]});
            }
            _ => {
                // swap fields between two different submissions
                let mut other = serde_json::to_value(&sealed[rng.below(3)]).unwrap();
                std::mem::swap(obj.get_mut("fragment").unwrap(), other.get_mut("fragment").unwrap());
            }
        }
        let result = std::panic::catch_unwind(|| threshold::FragmentSubmission::parse(value.clone()));
        match result {
            Ok(Ok(_)) => accepted += 1,
            Ok(Err(_)) => {}
            Err(_) => panic!("PANIC at round {round} on input: {value}"),
        }
        if round % 5000 == 4999 {
            eprintln!("round {round}: {accepted} mutated submissions still verified");
        }
    }
    println!("20k mutated submissions: {accepted} verified (expected ~0), no panic");
}

#[test]
fn cache_adversarial_sequences_hold_invariants() {
    let sealed_a = valid_seal();
    let sealed_b = valid_seal();
    let mut rng = Rng(0xBADA55);
    let mut cache = threshold::ThresholdCache::new(8).unwrap();
    let id_a = sealed_a[0].header.id.clone();
    let _id_b = sealed_b[0].header.id.clone();
    let mut decoded = std::collections::BTreeSet::new();
    let mut rejected = std::collections::BTreeSet::new();
    for round in 0..5_000u64 {
        let pick = rng.below(6);
        let source = if rng.below(2) == 0 { &sealed_a } else { &sealed_b };
        let mut sub = source[rng.below(3)].clone();
        let slot = 8 + round % 40;
        match pick {
            0 => { sub.index = (sub.index + 1) % 3; } // wrong index vs share
            1 => {
                // corrupt shard bytes: not covered by the header signature
                let mut raw = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, &sub.fragment).unwrap();
                if raw.len() > 17 {
                    let i = 17 + rng.below(raw.len() - 17);
                    raw[i] ^= 0xFF;
                }
                sub.fragment = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, raw);
            }
            2 => { sub.receiver = (sub.receiver % 3) + 1; } // maybe wrong receiver
            3 => {
                // splice header of A with fragment of B: conflicting candidate
                sub.header = sealed_b[0].header.clone();
                sub.header_signature = sealed_b[0].header_signature.clone();
            }
            4 => {
                cache.reject(&id_a);
                rejected.insert(id_a.clone());
            }
            _ => {}
        }
        let verified = match sub.verify() {
            Ok(v) => v,
            Err(_) => continue,
        };
        let id = verified.header.id.clone();
        let receiver = verified.receiver;
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            cache.accept_at(slot, 0, receiver, verified.clone())
        }));
        let result = match result {
            Ok(r) => r,
            Err(_) => panic!("PANIC at round {round}: {:?}", verified.header.id),
        };
        match &result {
            Ok(threshold::AcceptOutcome::Decoded { .. }) => {
                assert!(decoded.insert(id.clone()), "same id decoded twice");
                assert!(!rejected.contains(&id), "rejected id revived");
            }
            Ok(threshold::AcceptOutcome::AlreadyDecoded) => {
                assert!(decoded.contains(&id), "AlreadyDecoded for never-decoded id");
            }
            Ok(threshold::AcceptOutcome::Pending { .. }) => {
                assert!(!rejected.contains(&id), "rejected id accepted again");
            }
            Err(_) => {}
        }
        if round % 97 == 0 {
            for expired in cache.expire(slot) {
                rejected.insert(expired);
            }
        }
    }
    println!("5k adversarial cache operations: invariants held, no panic");
      }
