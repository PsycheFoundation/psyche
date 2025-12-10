use anchor_lang::prelude::{borsh::BorshSerialize, *};
use anchor_lang_idl::{
    build::IdlBuild,
    types::{
        IdlArrayLen, IdlDefinedFields, IdlField, IdlRepr, IdlReprModifier, IdlType, IdlTypeDef,
        IdlTypeDefTy,
    },
};
use bitvec::array::BitArray;
use bytemuck::Zeroable;
use fnv::FnvHasher;
use serde::{Deserialize, Deserializer, Serialize};
use std::{collections::BTreeMap, fmt, hash::Hasher};

// Modified from https://github.com/solana-labs/solana/blob/27eff8408b7223bb3c4ab70523f8a8dca3ca6645/bloom/src/bloom.rs

/// Generate a stable hash of `self` for each `hash_index`
/// Best effort can be made for uniqueness of each hash.
pub trait BloomHashIndex {
    fn hash_at_index(&self, hash_index: u64) -> u64;
}

#[derive(Clone, PartialEq, Eq, Copy, Zeroable)]
#[repr(C)]
pub struct Bloom<const U: usize, const K: usize> {
    pub keys: [u64; K],
    pub bits: BitArrayWrapper<U>,
}

#[derive(Clone, PartialEq, Eq, Copy, Default, Serialize, Deserialize)]
#[repr(transparent)]
pub struct BitArrayWrapper<const U: usize>(pub BitArray<[u64; U]>);

impl<const U: usize> AnchorSerialize for BitArrayWrapper<U> {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        let raw_data = self.0.as_raw_slice();
        for chunk in raw_data {
            AnchorSerialize::serialize(chunk, writer)?;
        }
        Ok(())
    }
}

impl<const U: usize> AnchorDeserialize for BitArrayWrapper<U> {
    fn deserialize_reader<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let mut data = [0u64; U];
        for chunk in &mut data {
            *chunk = u64::deserialize_reader(reader)?;
        }
        Ok(BitArrayWrapper(BitArray::new(data)))
    }
}

impl<const U: usize> IdlBuild for BitArrayWrapper<U> {
    fn create_type() -> Option<IdlTypeDef> {
        Some(IdlTypeDef {
            name: format!("BitArrayWrapper{U}").to_string(),
            docs: vec!["A wrapper around BitArray for serialization".to_string()],
            serialization: Default::default(),
            repr: Some(IdlRepr::Transparent),
            generics: vec![],
            ty: IdlTypeDefTy::Struct {
                fields: Some(IdlDefinedFields::Named(vec![IdlField {
                    name: "0".to_string(),
                    docs: vec!["The underlying bit array".to_string()],
                    ty: IdlType::Array(Box::new(IdlType::U64), IdlArrayLen::Value(U)),
                }])),
            },
        })
    }

    fn insert_types(_types: &mut BTreeMap<String, IdlTypeDef>) {
        // no inner types in idl
    }

    fn get_full_path() -> String {
        format!("{}::BitArrayWrapper{}", module_path!(), U)
    }
}

unsafe impl<const U: usize> Zeroable for BitArrayWrapper<U> {}

impl<const U: usize> BitArrayWrapper<U> {
    pub fn new(bits_data: [u64; U]) -> Self {
        Self(BitArray::new(bits_data))
    }
}

impl<const U: usize, const K: usize> Default for Bloom<U, K> {
    fn default() -> Self {
        Self {
            keys: [0u64; K],
            bits: Default::default(),
        }
    }
}

impl<const M: usize, const K: usize> Serialize for Bloom<M, K> {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut state = serializer.serialize_struct("Bloom", 3)?;
        state.serialize_field("keys", &self.keys.to_vec())?;
        state.serialize_field("bits", &self.bits)?;
        state.end()
    }
}

impl<'de, const U: usize, const K: usize> Deserialize<'de> for Bloom<U, K> {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct BloomHelper<const U: usize> {
            keys: Vec<u64>,
            bits: BitArrayWrapper<U>,
        }

        let helper = BloomHelper::deserialize(deserializer)?;

        if helper.keys.len() != K {
            return Err(serde::de::Error::custom(format!(
                "Expected {} keys, got {}",
                K,
                helper.keys.len()
            )));
        }

        let mut keys = [0u64; K];
        keys.copy_from_slice(&helper.keys);

        Ok(Bloom {
            keys,
            bits: helper.bits,
        })
    }
}

impl<const U: usize, const K: usize> AnchorSerialize for Bloom<U, K> {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        for key in &self.keys {
            AnchorSerialize::serialize(key, writer)?;
        }
        BorshSerialize::serialize(&self.bits, writer)
    }
}

impl<const U: usize, const K: usize> AnchorDeserialize for Bloom<U, K> {
    fn deserialize_reader<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let mut keys = [0u64; K];
        for key in &mut keys {
            *key = u64::deserialize_reader(reader)?;
        }
        let bits = BitArrayWrapper::deserialize_reader(reader)?;
        Ok(Bloom { keys, bits })
    }
}

impl<const U: usize, const K: usize> IdlBuild for Bloom<U, K> {
    fn create_type() -> Option<IdlTypeDef> {
        Some(IdlTypeDef {
            name: format!("Bloom{U}_{K}"),
            docs: vec![
                "A Bloom filter implementation with configurable size and number of hash functions"
                    .to_string(),
            ],
            serialization: Default::default(),
            repr: Some(IdlRepr::C(IdlReprModifier {
                packed: false,
                align: None,
            })),
            generics: vec![],
            ty: IdlTypeDefTy::Struct {
                fields: Some(IdlDefinedFields::Named(vec![
                    IdlField {
                        name: "keys".to_string(),
                        docs: vec!["Hash function keys".to_string()],
                        ty: IdlType::Array(Box::new(IdlType::U64), IdlArrayLen::Value(K)),
                    },
                    IdlField {
                        name: "bits".to_string(),
                        docs: vec!["Bit array for the Bloom filter".to_string()],
                        ty: IdlType::Defined {
                            name: format!("BitArrayWrapper{U}"),
                            generics: vec![],
                        },
                    },
                ])),
            },
        })
    }

    fn insert_types(types: &mut BTreeMap<String, IdlTypeDef>) {
        if let Some(ty) = BitArrayWrapper::<U>::create_type() {
            types.insert(ty.name.clone(), ty);
        }
    }

    fn get_full_path() -> String {
        format!("{}::Bloom{}_{}", module_path!(), U, K)
    }
}

impl<const U: usize, const K: usize> fmt::Debug for Bloom<U, K> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Bloom {{ keys.len: {}, bits: ", self.keys.len())?;
        const MAX_PRINT_BITS: usize = 10;

        if Self::max_bits() <= MAX_PRINT_BITS {
            // Print individual bits for small filters
            for i in 0..Self::max_bits() {
                match self.bits.0.get(i) {
                    Some(x) => write!(f, "{}", *x as u8)?,
                    None => write!(f, "X")?,
                }
            }
        } else {
            // Print byte array for larger filters
            let words = self.bits.0.as_raw_slice();
            for byte in words.iter() {
                write!(f, "{byte:016x}")?; // full u64 output
            }
        }

        write!(f, " }}")
    }
}

impl<const U: usize, const K: usize> Bloom<U, K> {
    pub const fn max_bits() -> usize {
        U * std::mem::size_of::<u64>() * 8
    }

    pub fn new(num_bits: usize, keys_slice: &[u64]) -> Self {
        assert!(num_bits <= Self::max_bits());
        assert!(keys_slice.len() == K);
        let mut keys = [0u64; K];
        let keys_2 = [0u64; U];
        keys.copy_from_slice(keys_slice);
        let bits = BitArrayWrapper::new(keys_2);
        Bloom { keys, bits }
    }

    /// Create filter optimal for num size given the `FALSE_RATE`.
    ///
    /// The keys are randomized for picking data out of a collision resistant hash of size
    /// `keysize` bytes.
    ///
    /// See <https://hur.st/bloomfilter/>.
    #[cfg(feature = "rand")]
    pub fn random(num_items: usize, false_rate: f64) -> Self {
        use rand::Rng;
        let m = Self::num_bits(num_items as f64, false_rate);
        let num_bits = std::cmp::max(1, std::cmp::min(m as usize, Self::max_bits()));
        let keys: Vec<u64> = (0..K).map(|_| rand::rng().random()).collect();
        Self::new(num_bits, &keys)
    }

    #[cfg(feature = "rand")]
    fn num_bits(num_items: f64, false_rate: f64) -> f64 {
        let n = num_items;
        let p = false_rate;
        ((n * p.ln()) / (1f64 / 2f64.powf(2f64.ln())).ln()).ceil()
    }

    #[cfg(feature = "rand")]
    #[allow(dead_code)]
    fn num_keys(num_bits: f64, num_items: f64) -> f64 {
        let n = num_items;
        let m = num_bits;
        // infinity as usize is zero in rust 1.43 but 2^64-1 in rust 1.45; ensure it's zero here
        if n == 0.0 {
            0.0
        } else {
            1f64.max(((m / n) * 2f64.ln()).round())
        }
    }

    fn pos<T: BloomHashIndex>(&self, key: &T, k: u64) -> u64 {
        key.hash_at_index(k)
            .checked_rem(self.bits.0.len() as u64)
            .unwrap_or(0)
    }

    pub fn clear(&mut self) {
        let keys_2 = [0u64; U];
        let bits = BitArrayWrapper::new(keys_2);
        self.bits = bits;
    }

    pub fn add<T: BloomHashIndex>(&mut self, key: &T) {
        for k in &self.keys {
            let pos = self.pos(key, *k) as usize;
            if !*self.bits.0.get(pos).unwrap() {
                self.bits.0.set(pos, true);
            }
        }
    }

    pub fn contains<T: BloomHashIndex>(&self, key: &T) -> bool {
        for k in &self.keys {
            let pos = self.pos(key, *k) as usize;
            if !*self.bits.0.get(pos).unwrap() {
                return false;
            }
        }
        true
    }
}

fn slice_hash(slice: &[u8], hash_index: u64) -> u64 {
    let mut hasher = FnvHasher::with_key(hash_index);
    hasher.write(slice);
    hasher.finish()
}

impl<T: AsRef<[u8]>> BloomHashIndex for T {
    fn hash_at_index(&self, hash_index: u64) -> u64 {
        slice_hash(self.as_ref(), hash_index)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bloom_new() {
        let keys = [1, 2, 3];
        let bloom = Bloom::<16, 3>::new(100, &keys);

        assert_eq!(bloom.keys, keys);
    }

    #[test]
    fn test_bloom_add_and_contains() {
        let mut bloom = Bloom::<8, 2>::new(100, &[1, 2]);

        let item1 = vec![1, 2, 3];
        let item2 = vec![4, 5, 6];

        bloom.add(&item1);
        assert!(bloom.contains(&item1));
        assert!(!bloom.contains(&item2));
    }

    #[test]
    fn test_bloom_clear() {
        let mut bloom = Bloom::<8, 2>::new(100, &[1, 2]);

        let item = vec![1, 2, 3];
        bloom.add(&item);
        assert!(bloom.contains(&item));

        bloom.clear();
        assert!(!bloom.contains(&item));
    }

    #[test]
    fn test_multiple_items() {
        let mut bloom = Bloom::<16, 3>::new(1000, &[1, 2, 3]);

        let items = vec![vec![1, 2, 3], vec![4, 5, 6], vec![7, 8, 9]];

        for item in &items {
            bloom.add(item);
        }

        for item in &items {
            assert!(bloom.contains(item));
        }

        let non_existing = vec![1, 4, 7];
        assert!(!bloom.contains(&non_existing));
    }
}
