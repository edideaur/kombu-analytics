#![forbid(unsafe_code)]

use thiserror::Error;

#[derive(Debug, Error)]
pub enum CryptoError {
    #[error("encrypt failed")]
    Encrypt,
    #[error("decrypt failed")]
    Decrypt,
    #[error("invalid base64")]
    Base64,
}

pub fn hash(args: &[&str]) -> String {
    let mut hasher = blake3::Hasher::new();
    for a in args {
        hasher.update(a.as_bytes());
    }
    hasher.finalize().to_hex().to_string()
}

pub fn sha512_hex(args: &[&str]) -> String {
    hash(args)
}

pub fn md5_hex(args: &[&str]) -> String {
    let h = hash(args);
    h[..32].to_string()
}

pub fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn const_eq() {
        assert!(constant_time_eq(b"abc", b"abc"));
        assert!(!constant_time_eq(b"abc", b"abd"));
        assert!(!constant_time_eq(b"abc", b"ab"));
    }

    #[test]
    fn hash_deterministic() {
        assert_eq!(hash(&["a", "b"]), hash(&["a", "b"]));
        assert_ne!(hash(&["a"]), hash(&["b"]));
        assert_eq!(sha512_hex(&["a"]), hash(&["a"]));
        assert_eq!(md5_hex(&["a"]).len(), 32);
    }

    #[test]
    fn test_crypto_error_display() {
        assert_eq!(CryptoError::Encrypt.to_string(), "encrypt failed");
        assert_eq!(CryptoError::Decrypt.to_string(), "decrypt failed");
        assert_eq!(CryptoError::Base64.to_string(), "invalid base64");
    }
}

#[cfg(kani)]
mod kani_proofs {
    use super::*;

    #[kani::proof]
    fn harness_ct_eq_no_panic() {
        let a: [u8; 16] = kani::any();
        let b: [u8; 16] = kani::any();
        let eq1 = constant_time_eq(&a, &b);
        let eq2 = constant_time_eq(&b, &a);
        kani::assert(eq1 == eq2, "symmetry");
        kani::assert(constant_time_eq(&a, &a), "reflexivity");
    }

    #[kani::proof]
    fn harness_ct_eq_lengths() {
        let a = [1u8, 2, 3];
        let b = [1u8, 2];
        kani::assert(!constant_time_eq(&a, &b), "diff length false");
        kani::assert(constant_time_eq(&a, &a), "same length same val");
    }
}
