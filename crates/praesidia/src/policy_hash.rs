use sha2::{Digest, Sha256};

pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    format!("{:x}", h.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_input_produces_known_hash() {
        // SHA-256 of empty input is a well-known constant
        let hash = sha256_hex(b"");
        assert_eq!(hash, "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");
    }

    #[test]
    fn known_input_produces_correct_hash() {
        // SHA-256("hello") is well-known
        let hash = sha256_hex(b"hello");
        assert_eq!(hash, "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824");
    }

    #[test]
    fn hash_is_lowercase_hex() {
        let hash = sha256_hex(b"test");
        assert!(hash.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
    }

    #[test]
    fn hash_length_is_64() {
        let hash = sha256_hex(b"any input");
        assert_eq!(hash.len(), 64);
    }

    #[test]
    fn different_inputs_produce_different_hashes() {
        let h1 = sha256_hex(b"input1");
        let h2 = sha256_hex(b"input2");
        assert_ne!(h1, h2);
    }

    #[test]
    fn same_input_produces_same_hash() {
        let h1 = sha256_hex(b"consistent");
        let h2 = sha256_hex(b"consistent");
        assert_eq!(h1, h2);
    }
}
