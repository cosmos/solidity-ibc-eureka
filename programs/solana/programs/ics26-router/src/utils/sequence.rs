use anchor_lang::prelude::*;
use solana_sha256_hasher::hashv;

/// Derives a deterministic suffix in range [0, 9999] from `hash(calling_program || sender)`.
pub fn derive_sequence_suffix(calling_program: &Pubkey, sender: &Pubkey) -> u16 {
    let hash = hashv(&[calling_program.as_ref(), sender.as_ref()]);
    let bytes = hash.to_bytes();
    let raw_u16 = u16::from_le_bytes([bytes[0], bytes[1]]);
    raw_u16 % 10000
}

/// Calculates namespaced sequence: `base_sequence * 10000 + suffix`.
/// Creates unique sequence ranges per (program, sender) pair for collision resistance.
pub fn calculate_namespaced_sequence(
    base_sequence: u64,
    calling_program: &Pubkey,
    sender: &Pubkey,
) -> Result<u64> {
    let suffix = u64::from(derive_sequence_suffix(calling_program, sender));

    base_sequence
        .checked_mul(10000)
        .and_then(|v| v.checked_add(suffix))
        .ok_or_else(|| error!(crate::errors::RouterError::ArithmeticOverflow))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_derive_sequence_suffix() {
        let program = Pubkey::new_unique();
        let sender = Pubkey::new_unique();

        let suffix1 = derive_sequence_suffix(&program, &sender);
        let suffix2 = derive_sequence_suffix(&program, &sender);

        assert_eq!(suffix1, suffix2);
        assert!(suffix1 < 10000);
    }

    #[test]
    fn test_calculate_namespaced_sequence() {
        let program = Pubkey::new_unique();
        let sender = Pubkey::new_unique();

        let seq1 = calculate_namespaced_sequence(1, &program, &sender).unwrap();
        let seq2 = calculate_namespaced_sequence(2, &program, &sender).unwrap();

        assert!((10000..20000).contains(&seq1));
        assert!((20000..30000).contains(&seq2));
        assert_eq!(seq1 % 10000, seq2 % 10000);
    }
}
