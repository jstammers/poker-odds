//! Lookup tables for O(1) 5-card hand evaluation.
//!
//! Based on the Cactus Kev evaluator approach:
//! - Flush detection via a rank bitmask table
//! - Unique-5 non-flush hands (straights + high cards) via rank bitmask
//! - Paired/trips/quads hands via prime product hashing
//!
//! Tables are built once at program startup and cached.

use crate::cards::card::{Card, Rank};
use once_cell::sync::Lazy;

/// Straight hand table: (rank_mask, straight_ck_rank, straight_flush_ck_rank)
/// rank_mask uses bit-per-rank (Two=bit0 … Ace=bit12).
/// straight_ck_rank: CK rank for a non-flush straight (1600–1609).
/// straight_flush_ck_rank: CK rank for the same straight when suited (1–10).
const STRAIGHTS: [(u16, u16, u16); 10] = [
    (7936, 1600, 1),  // A-K-Q-J-T: Broadway / Royal Flush
    (3968, 1601, 2),  // K-Q-J-T-9
    (1984, 1602, 3),  // Q-J-T-9-8
    (992, 1603, 4),   // J-T-9-8-7
    (496, 1604, 5),   // T-9-8-7-6
    (248, 1605, 6),   // 9-8-7-6-5
    (124, 1606, 7),   // 8-7-6-5-4
    (62, 1607, 8),    // 7-6-5-4-3
    (31, 1608, 9),    // 6-5-4-3-2
    (4111, 1609, 10), // A-5-4-3-2 (wheel / steel wheel)
];

/// For non-paired 5-card hands, look up the CK rank from the rank bitmask.
/// Index is the 13-bit rank presence bitmask.
pub static UNIQUE5_TABLE: Lazy<[u16; 8192]> = Lazy::new(build_unique5_table);

/// For flush hands, look up the CK rank from the rank bitmask.
/// Index is the 13-bit rank presence bitmask.
pub static FLUSH_TABLE: Lazy<[u16; 8192]> = Lazy::new(build_flush_table);

/// For paired/trips/quads hands: sorted (prime_product, ck_rank) pairs.
/// Look up with `lookup_pairs(product)` which binary-searches in O(log n).
pub static PAIRS_TABLE: Lazy<Box<[(u32, u16)]>> = Lazy::new(|| {
    let mut v = build_pairs_vec();
    v.sort_unstable_by_key(|&(k, _)| k);
    v.into_boxed_slice()
});

/// Binary-search the pairs table for the given prime product. Panics on invalid input.
#[inline]
pub fn lookup_pairs(product: u32) -> u16 {
    let table = &*PAIRS_TABLE;
    match table.binary_search_by_key(&product, |&(k, _)| k) {
        Ok(idx) => table[idx].1,
        Err(_) => panic!("prime product {product} not found in pairs table — invalid hand"),
    }
}

fn build_unique5_table() -> [u16; 8192] {
    let mut table = [0u16; 8192];
    // Straights: assign correct non-flush straight CK ranks (1600–1609)
    for &(mask, straight_ck, _sf_ck) in &STRAIGHTS {
        if mask < 8192 {
            table[mask as usize] = straight_ck;
        }
    }
    // High cards: enumerate all C(13,5) non-straight combos, assign CK 6186–7462
    let mut high_card_hands: Vec<u16> = Vec::new();
    for combo in combinations_5_of_13() {
        let mask = combo_to_mask(&combo);
        if !is_straight(mask) {
            high_card_hands.push(mask);
        }
    }
    // Sort descending by mask value — higher mask = stronger hand (Ace is bit 12)
    high_card_hands.sort_by(|&a, &b| b.cmp(&a));
    for (i, mask) in high_card_hands.iter().enumerate() {
        table[*mask as usize] = 6186 + i as u16;
    }
    table
}

fn build_flush_table() -> [u16; 8192] {
    let mut table = [0u16; 8192];
    // Straight flushes: assign SF CK ranks (1=Royal, 2–10 for others)
    for &(mask, _straight_ck, sf_ck) in &STRAIGHTS {
        if mask < 8192 {
            table[mask as usize] = sf_ck;
        }
    }
    // Regular flushes: CK ranks 323–1599
    let mut flush_hands: Vec<u16> = Vec::new();
    for combo in combinations_5_of_13() {
        let mask = combo_to_mask(&combo);
        if !is_straight(mask) {
            flush_hands.push(mask);
        }
    }
    flush_hands.sort_by(|&a, &b| b.cmp(&a));
    for (i, mask) in flush_hands.iter().enumerate() {
        table[*mask as usize] = 323 + i as u16;
    }
    table
}

fn build_pairs_vec() -> Vec<(u32, u16)> {
    let mut map: Vec<(u32, u16)> = Vec::with_capacity(4888);
    // Generate all possible 5-card multisets with pairs/trips/quads
    // We enumerate by rank counts (e.g., [4,1,0,...], [3,2,0,...], etc.)
    // and assign CK ranks based on hand category and rank ordering

    // Four of a Kind: C(13,1) * 12 = 156 hands, CK ranks 11-166
    let mut foak: Vec<(Rank, Rank)> = Vec::new(); // (quad rank, kicker)
    for &qr in &Rank::ALL {
        for &kr in &Rank::ALL {
            if kr != qr {
                foak.push((qr, kr));
            }
        }
    }
    // Sort: higher quad rank first, then higher kicker
    foak.sort_by(|a, b| b.0.cmp(&a.0).then(b.1.cmp(&a.1)));
    for (i, (qr, kr)) in foak.iter().enumerate() {
        let product = qr.prime().pow(4) * kr.prime();
        map.push((product, 11 + i as u16));
    }

    // Full House: 13 * 12 = 156 hands, CK ranks 167-322
    let mut fh: Vec<(Rank, Rank)> = Vec::new(); // (trips rank, pair rank)
    for &tr in &Rank::ALL {
        for &pr in &Rank::ALL {
            if pr != tr {
                fh.push((tr, pr));
            }
        }
    }
    fh.sort_by(|a, b| b.0.cmp(&a.0).then(b.1.cmp(&a.1)));
    for (i, (tr, pr)) in fh.iter().enumerate() {
        let product = tr.prime().pow(3) * pr.prime().pow(2);
        map.push((product, 167 + i as u16));
    }

    // Three of a Kind: C(13,1) * C(12,2) = 858 hands, CK ranks 1610-2467
    let mut toak: Vec<(Rank, [Rank; 2])> = Vec::new();
    for (i, &tr) in Rank::ALL.iter().enumerate() {
        for j in 0..13usize {
            for k in (j + 1)..13 {
                let k1 = Rank::ALL[j];
                let k2 = Rank::ALL[k];
                if k1 != tr && k2 != tr {
                    // kickers sorted descending
                    let kickers = if k1 > k2 { [k1, k2] } else { [k2, k1] };
                    toak.push((tr, kickers));
                }
            }
        }
        let _ = i;
    }
    toak.sort_by(|a, b| {
        b.0.cmp(&a.0)
            .then(b.1[0].cmp(&a.1[0]))
            .then(b.1[1].cmp(&a.1[1]))
    });
    for (i, (tr, kickers)) in toak.iter().enumerate() {
        let product = tr.prime().pow(3) * kickers[0].prime() * kickers[1].prime();
        map.push((product, 1610 + i as u16));
    }

    // Two Pair: C(13,2) * 11 = 858 hands, CK ranks 2468-3325
    let mut tp: Vec<([Rank; 2], Rank)> = Vec::new();
    for i in 0..13usize {
        for j in (i + 1)..13 {
            let p1 = Rank::ALL[i];
            let p2 = Rank::ALL[j];
            let pairs = if p1 > p2 { [p1, p2] } else { [p2, p1] };
            for &kr in &Rank::ALL {
                if kr != p1 && kr != p2 {
                    tp.push((pairs, kr));
                }
            }
        }
    }
    tp.sort_by(|a, b| {
        b.0[0]
            .cmp(&a.0[0])
            .then(b.0[1].cmp(&a.0[1]))
            .then(b.1.cmp(&a.1))
    });
    for (i, (pairs, kr)) in tp.iter().enumerate() {
        let product = pairs[0].prime().pow(2) * pairs[1].prime().pow(2) * kr.prime();
        map.push((product, 2468 + i as u16));
    }

    // One Pair: C(13,1) * C(12,3) = 2860 hands, CK ranks 3326-6185
    let mut op: Vec<(Rank, [Rank; 3])> = Vec::new();
    for &pr in &Rank::ALL {
        // Choose 3 kickers from the remaining 12 ranks
        let others: Vec<Rank> = Rank::ALL.iter().filter(|&&r| r != pr).cloned().collect();
        for i in 0..others.len() {
            for j in (i + 1)..others.len() {
                for k in (j + 1)..others.len() {
                    let mut kickers = [others[i], others[j], others[k]];
                    kickers.sort_by(|a, b| b.cmp(a)); // descending
                    op.push((pr, kickers));
                }
            }
        }
    }
    op.sort_by(|a, b| {
        b.0.cmp(&a.0)
            .then(b.1[0].cmp(&a.1[0]))
            .then(b.1[1].cmp(&a.1[1]))
            .then(b.1[2].cmp(&a.1[2]))
    });
    for (i, (pr, kickers)) in op.iter().enumerate() {
        let product =
            pr.prime().pow(2) * kickers[0].prime() * kickers[1].prime() * kickers[2].prime();
        map.push((product, 3326 + i as u16));
    }

    map
}

fn combinations_5_of_13() -> Vec<[usize; 5]> {
    let mut result = Vec::new();
    for a in 0..9 {
        for b in (a + 1)..10 {
            for c in (b + 1)..11 {
                for d in (c + 1)..12 {
                    for e in (d + 1)..13 {
                        result.push([a, b, c, d, e]);
                    }
                }
            }
        }
    }
    result
}

fn combo_to_mask(combo: &[usize; 5]) -> u16 {
    combo.iter().fold(0u16, |acc, &i| acc | (1 << i))
}

fn is_straight(mask: u16) -> bool {
    STRAIGHTS.iter().any(|&(m, _, _)| m == mask)
}

/// Compute the rank bitmask of 5 cards (bit i = rank index i is present).
pub fn rank_mask_of(cards: &[Card; 5]) -> u16 {
    cards
        .iter()
        .fold(0u16, |acc, c| acc | (1 << c.rank.index()))
}

/// Check if all 5 cards share the same suit.
pub fn is_flush(cards: &[Card; 5]) -> bool {
    let s = cards[0].suit;
    cards[1..].iter().all(|c| c.suit == s)
}

/// Compute the prime product of 5 card ranks.
pub fn prime_product(cards: &[Card; 5]) -> u32 {
    cards.iter().map(|c| c.rank.prime()).product()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tables_initialize() {
        let _ = &*UNIQUE5_TABLE;
        let _ = &*FLUSH_TABLE;
        let _ = &*PAIRS_TABLE;
    }

    #[test]
    fn pairs_table_one_pair_count() {
        // One pair: 2860 entries
        let op_count = PAIRS_TABLE
            .iter()
            .filter(|&&(_, v)| (3326..=6185).contains(&v))
            .count();
        assert_eq!(
            op_count, 2860,
            "expected 2860 one-pair hands, got {op_count}"
        );
    }

    #[test]
    fn pairs_table_total_count() {
        // 156 + 156 + 858 + 858 + 2860 = 4888
        assert_eq!(PAIRS_TABLE.len(), 4888, "expected 4888 paired hands");
    }

    #[test]
    fn pairs_table_is_sorted() {
        let table = &*PAIRS_TABLE;
        for w in table.windows(2) {
            assert!(
                w[0].0 < w[1].0,
                "PAIRS_TABLE not sorted at product {}",
                w[0].0
            );
        }
    }
}
