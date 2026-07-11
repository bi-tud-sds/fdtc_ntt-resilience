#![allow(clippy::needless_range_loop)]

use crate::modarith::{inv_mod, mul_mod, pow_mod};
use crate::ntt::{intt, mul_ntt, ntt};
use crate::params::RingParams;

/// Reference O(N^2) negacyclic multiplication retained for validation experiments.
#[allow(dead_code)]
pub fn naive_negacyclic_mul(a: &[u64], b: &[u64], q: u64) -> Vec<u64> {
    let n = a.len();
    let mut out = vec![0u64; n];
    for i in 0..n {
        for j in 0..n {
            let prod = mul_mod(a[i], b[j], q);
            let k = i + j;
            if k < n {
                out[k] = ((out[k] as u128 + prod as u128) % q as u128) as u64;
            } else {
                out[k - n] = ((out[k - n] as u128 + q as u128 - prod as u128) % q as u128) as u64;
            }
        }
    }
    out
}

/// Reference negacyclic multiplication via NTT retained for validation experiments.
#[allow(dead_code)]
pub fn negacyclic_ntt_mul(a: &[u64], b: &[u64], params: &RingParams) -> Vec<u64> {
    let n = params.n;
    let q = params.modulus;
    let psi = params.primitive_2n_root;
    let psi_inv = inv_mod(psi, q);
    let mut ta = vec![0u64; n];
    let mut tb = vec![0u64; n];

    for i in 0..n {
        ta[i] = mul_mod(a[i], pow_mod(psi, i as u64, q), q);
        tb[i] = mul_mod(b[i], pow_mod(psi, i as u64, q), q);
    }

    let (a_hat, _) = ntt(&ta, params, false);
    let (b_hat, _) = ntt(&tb, params, false);
    let c_hat = mul_ntt(&a_hat, &b_hat, q);
    let (mut c, _) = intt(&c_hat, params, false);

    for i in 0..n {
        c[i] = mul_mod(c[i], pow_mod(psi_inv, i as u64, q), q);
    }
    c
}
