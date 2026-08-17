//! A small deterministic random number generator.
//!
//! Written out rather than pulling in `rand`, for two reasons. The simulator needs to be
//! **reproducible** — the same deck and seed must give the same numbers, or a score would
//! wobble between runs and the optimizer would chase noise. And the `rand` ecosystem is a
//! sizeable dependency tree for what amounts to shuffling a 100-card list.
//!
//! This is PCG-XSH-RR, the standard 64-to-32 variant. It is not cryptographic and does not
//! need to be.

/// A seeded generator.
#[derive(Debug, Clone)]
pub struct Rng {
    state: u64,
    increment: u64,
}

// PCG's published constants.
const MULTIPLIER: u64 = 6_364_136_223_846_793_005;
const DEFAULT_INCREMENT: u64 = 1_442_695_040_888_963_407;

impl Rng {
    /// Creates a generator from a seed. The same seed always gives the same sequence.
    pub fn new(seed: u64) -> Rng {
        // The increment must be odd for the LCG to reach full period.
        let mut rng = Rng {
            state: 0,
            increment: DEFAULT_INCREMENT | 1,
        };
        rng.next_u32();
        rng.state = rng.state.wrapping_add(seed);
        rng.next_u32();
        rng
    }

    fn next_u32(&mut self) -> u32 {
        let previous = self.state;
        self.state = previous
            .wrapping_mul(MULTIPLIER)
            .wrapping_add(self.increment);

        let xorshifted = (((previous >> 18) ^ previous) >> 27) as u32;
        let rotation = (previous >> 59) as u32;
        xorshifted.rotate_right(rotation)
    }

    /// A uniform integer in `0..bound`. Returns 0 when `bound` is 0.
    ///
    /// Rejection-sampled rather than taking a modulus: the naive version is biased towards
    /// small values, which for a shuffle means the deck is not actually shuffled.
    pub fn below(&mut self, bound: u32) -> u32 {
        if bound == 0 {
            return 0;
        }
        let threshold = bound.wrapping_neg() % bound;
        loop {
            let value = self.next_u32();
            if value >= threshold {
                return value % bound;
            }
        }
    }

    /// A uniform float in `[0, 1)`.
    pub fn unit(&mut self) -> f64 {
        // 24 bits is the mantissa of an f32 and plenty for accept/reject decisions.
        f64::from(self.next_u32() >> 8) / f64::from(1u32 << 24)
    }

    /// Fisher-Yates, which is the only shuffle that is actually uniform.
    pub fn shuffle<T>(&mut self, items: &mut [T]) {
        for i in (1..items.len()).rev() {
            let j = self.below(i as u32 + 1) as usize;
            items.swap(i, j);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_same_seed_gives_the_same_sequence() {
        // The property the whole simulator rests on: a score must not wobble between runs.
        let mut a = Rng::new(42);
        let mut b = Rng::new(42);
        for _ in 0..1000 {
            assert_eq!(a.next_u32(), b.next_u32());
        }
    }

    #[test]
    fn different_seeds_diverge() {
        let mut a = Rng::new(1);
        let mut b = Rng::new(2);
        let differences = (0..100).filter(|_| a.next_u32() != b.next_u32()).count();
        assert!(differences > 90, "only {differences} of 100 differed");
    }

    #[test]
    fn bounded_values_stay_in_range() {
        let mut rng = Rng::new(7);
        for bound in [1u32, 2, 7, 52, 100, 1000] {
            for _ in 0..2000 {
                assert!(rng.below(bound) < bound);
            }
        }
        assert_eq!(rng.below(0), 0, "a zero bound must not divide by zero");
    }

    #[test]
    fn small_bounds_are_not_biased() {
        // The reason for rejection sampling. A modulus would skew towards low values, and a
        // skewed shuffle means the "random" opening hand is not random.
        let mut rng = Rng::new(99);
        let mut counts = [0u32; 3];
        const DRAWS: u32 = 60_000;
        for _ in 0..DRAWS {
            counts[rng.below(3) as usize] += 1;
        }
        for count in counts {
            let share = f64::from(count) / f64::from(DRAWS);
            assert!((share - 1.0 / 3.0).abs() < 0.01, "{counts:?}");
        }
    }

    #[test]
    fn unit_values_are_in_range_and_spread_out() {
        let mut rng = Rng::new(3);
        let mut buckets = [0u32; 10];
        for _ in 0..20_000 {
            let value = rng.unit();
            assert!((0.0..1.0).contains(&value), "{value}");
            buckets[(value * 10.0) as usize] += 1;
        }
        for count in buckets {
            assert!(count > 1500, "{buckets:?}");
        }
    }

    #[test]
    fn shuffling_keeps_every_card() {
        let mut rng = Rng::new(11);
        let mut deck: Vec<u32> = (0..100).collect();
        rng.shuffle(&mut deck);

        assert_eq!(deck.len(), 100);
        let mut sorted = deck.clone();
        sorted.sort_unstable();
        assert_eq!(sorted, (0..100).collect::<Vec<u32>>());
        assert_ne!(
            deck, sorted,
            "a shuffle that changes nothing is not a shuffle"
        );
    }

    #[test]
    fn shuffling_moves_cards_away_from_where_they_started() {
        // A weak shuffle would leave most cards near their original index.
        let mut rng = Rng::new(5);
        let mut total_displacement = 0usize;
        for _ in 0..50 {
            let mut deck: Vec<usize> = (0..60).collect();
            rng.shuffle(&mut deck);
            total_displacement += deck
                .iter()
                .enumerate()
                .map(|(position, card)| position.abs_diff(*card))
                .sum::<usize>();
        }
        let average = total_displacement as f64 / (50.0 * 60.0);
        assert!(average > 15.0, "average displacement was only {average}");
    }

    #[test]
    fn shuffling_a_tiny_slice_does_not_panic() {
        let mut rng = Rng::new(1);
        rng.shuffle::<u32>(&mut []);
        rng.shuffle(&mut [1]);
    }
}
