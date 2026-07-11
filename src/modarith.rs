pub fn add_mod(a: u64, b: u64, q: u64) -> u64 {
    ((a as u128 + b as u128) % q as u128) as u64
}

pub fn sub_mod(a: u64, b: u64, q: u64) -> u64 {
    if a >= b {
        a - b
    } else {
        q - (b - a)
    }
}

pub fn mul_mod(a: u64, b: u64, q: u64) -> u64 {
    ((a as u128 * b as u128) % q as u128) as u64
}

pub fn pow_mod(mut base: u64, mut exp: u64, q: u64) -> u64 {
    let mut out = 1u64;
    while exp > 0 {
        if exp & 1 == 1 {
            out = mul_mod(out, base, q);
        }
        base = mul_mod(base, base, q);
        exp >>= 1;
    }
    out
}

pub fn inv_mod(a: u64, q: u64) -> u64 {
    pow_mod(a, q - 2, q)
}

pub fn centered(x: u64, q: u64) -> i128 {
    if x <= q / 2 {
        x as i128
    } else {
        x as i128 - q as i128
    }
}

pub fn from_centered(x: i128, q: u64) -> u64 {
    let q_i = q as i128;
    let mut y = x % q_i;
    if y < 0 {
        y += q_i;
    }
    y as u64
}
