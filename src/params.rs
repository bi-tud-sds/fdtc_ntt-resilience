use crate::modarith::{mul_mod, pow_mod};

#[derive(Debug, Clone)]
pub struct RingParams {
    pub n: usize,
    pub modulus: u64,
    pub modulus_bits: u32,
    pub primitive_2n_root: u64,
    pub primitive_n_root: u64,
}

impl RingParams {
    pub fn new(n: usize, modulus_bits: u32) -> Result<Self, String> {
        if !n.is_power_of_two() {
            return Err("N must be a power of two".to_string());
        }
        if !(4..=62).contains(&modulus_bits) {
            return Err("modulus_bits must be in [4, 62] for this demo implementation".to_string());
        }

        let modulus = find_ntt_prime(n, modulus_bits)
            .ok_or("could not find q ≡ 1 mod 2N prime".to_string())?;

        let primitive_2n_root = find_primitive_root_of_order(modulus, 2 * n)
            .ok_or("could not find primitive 2N-th root".to_string())?;

        let primitive_n_root = pow_mod(primitive_2n_root, 2, modulus);

        Ok(Self {
            n,
            modulus,
            modulus_bits,
            primitive_2n_root,
            primitive_n_root,
        })
    }
}

/// Finds a prime q with the requested bit width and q ≡ 1 mod 2N.
///
/// This routine is performance-critical for 50-62 bit experiments.  It uses
/// deterministic Miller-Rabin for u64 rather than trial division.
fn find_ntt_prime(n: usize, bits: u32) -> Option<u64> {
    let step = 2u64.checked_mul(n as u64)?;

    if bits >= 63 {
        return None;
    }

    let lo = 1u64 << (bits - 1);
    let hi = 1u64 << bits;

    // First candidate q >= lo such that q ≡ 1 mod step.
    let rem = lo % step;
    let add = if rem <= 1 { 1 - rem } else { step + 1 - rem };
    let mut q = lo.checked_add(add)?;

    while q < hi {
        if is_prime_u64(q) {
            return Some(q);
        }
        q = q.checked_add(step)?;
    }

    None
}

/// Deterministic Miller-Rabin primality test for all u64 inputs.
///
/// The witness set below is a known deterministic basis for 64-bit integers.
/// This avoids the previous near-64-bit bottleneck caused by trial division.
fn is_prime_u64(n: u64) -> bool {
    if n < 2 {
        return false;
    }

    const SMALL_PRIMES: [u64; 12] = [2, 3, 5, 7, 11, 13, 17, 19, 23, 29, 31, 37];
    for p in SMALL_PRIMES {
        if n == p {
            return true;
        }
        if n % p == 0 {
            return false;
        }
    }

    let mut d = n - 1;
    let mut s = 0u32;
    while d & 1 == 0 {
        d >>= 1;
        s += 1;
    }

    const WITNESSES: [u64; 7] = [2, 325, 9_375, 28_178, 450_775, 9_780_504, 1_795_265_022];

    'witness_loop: for a in WITNESSES {
        let a = a % n;
        if a == 0 {
            continue;
        }

        let mut x = pow_mod(a, d, n);
        if x == 1 || x == n - 1 {
            continue;
        }

        for _ in 1..s {
            x = mul_mod(x, x, n);
            if x == n - 1 {
                continue 'witness_loop;
            }
        }

        return false;
    }

    true
}

/// Finds a primitive root of exact order `order` in F_q.
///
/// q is selected so that order = 2N divides q-1.  Instead of scanning for an
/// element that already has order 2N, we project arbitrary field elements into
/// the order-2N subgroup:
///
///     root = h^((q-1)/order) mod q
///
/// For power-of-two `order`, exact order can be tested with one additional
/// check: root^(order/2) != 1.  This is dramatically faster for 60-62 bit q.
fn find_primitive_root_of_order(q: u64, order: usize) -> Option<u64> {
    let order_u64 = order as u64;
    if order < 2 || (q - 1) % order_u64 != 0 {
        return None;
    }

    let exponent = (q - 1) / order_u64;

    for h in 2..10_000u64 {
        let root = pow_mod(h, exponent, q);
        if root != 1 && pow_mod(root, order_u64, q) == 1 && pow_mod(root, order_u64 / 2, q) != 1 {
            return Some(root);
        }
    }

    // The probability of success is high, but keep a deterministic fallback.
    let mut h = 10_000u64;
    while h < q {
        let root = pow_mod(h, exponent, q);
        if root != 1 && pow_mod(root, order_u64, q) == 1 && pow_mod(root, order_u64 / 2, q) != 1 {
            return Some(root);
        }
        h += 1;
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_small_ntt_prime_and_roots() {
        let p = RingParams::new(16, 24).unwrap();
        assert_eq!(p.modulus % (2 * p.n as u64), 1);
        assert_eq!(pow_mod(p.primitive_2n_root, (2 * p.n) as u64, p.modulus), 1);
        assert_ne!(pow_mod(p.primitive_2n_root, p.n as u64, p.modulus), 1);
    }

    #[test]
    fn near_64_bit_prime_generation_is_fast_enough_for_tests() {
        let p = RingParams::new(16, 62).unwrap();
        assert!(p.modulus >= (1u64 << 61));
        assert_eq!(p.modulus % (2 * p.n as u64), 1);
        assert_eq!(pow_mod(p.primitive_2n_root, (2 * p.n) as u64, p.modulus), 1);
        assert_ne!(pow_mod(p.primitive_2n_root, p.n as u64, p.modulus), 1);
    }
}
