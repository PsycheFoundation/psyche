use anchor_lang::{
    prelude::borsh, // Removed Error as AnchorError
    AnchorDeserialize,
    AnchorSerialize,
};
use bytemuck::Zeroable;
use serde::{
    de::{SeqAccess, Visitor},
    ser::SerializeSeq,
    Deserialize, Deserializer, Serialize, Serializer,
};
use std::fmt;
use std::io::Read;

// If you use ts-rs, uncomment this and the derive on the struct
use ts_rs::TS;

// Helper const fn to calculate bytes needed for N elements of X bits each.
pub const fn packed_bytes_count(num_elements: usize, bits_per_element: usize) -> usize {
    if num_elements == 0 {
        0
    } else {
        (num_elements * bits_per_element + 7) / 8
    }
}

const BITS_PER_ELEMENT: usize = 6; // Each element is u6
const MAX_VALUE_U6: u8 = (1 << BITS_PER_ELEMENT) - 1; // 63
const CAPACITY: usize = 256; // Hardcoded capacity

/// Proxy object for mutable access to a u6 value in CompressedFixedVec.
pub struct U6RefMut<'a> {
    // Removed const N
    vec: &'a mut CompressedFixedVec, // Removed <N>
    idx: usize,
}

impl U6RefMut<'_> {
    /// Gets the current value at the referenced position.
    pub fn get(&self) -> u8 {
        // Unwrap is safe: U6RefMut should only be created for valid, in-bounds indices.
        self.vec.internal_get(self.idx).unwrap()
    }

    /// Sets a new value at the referenced position.
    /// Consumes the U6RefMut to mimic assignment semantics.
    pub fn set(self, value: u8) {
        if value > MAX_VALUE_U6 {
            panic!("Value out of u6 range (0-{})", MAX_VALUE_U6);
        }
        // Unwrap is safe: U6RefMut is valid, internal_set for valid idx won't err on value range.
        self.vec.internal_set(self.idx, value).unwrap();
    }
}

/// A fixed-size vector that stores u6 values (0-63) compactly.
/// Capacity is fixed by the `CAPACITY` const.
#[derive(Clone, Copy, Zeroable, TS)] // Zeroable assumes 0 len and 0 data is valid.
#[repr(C)] // Ensures field order for Zeroable.
#[ts(type = "Array<number>")]
pub struct CompressedFixedVec {
    // Removed const N
    // No where clause needed here anymore
    // Stores CAPACITY elements of BITS_PER_ELEMENT size.
    data: [u8; packed_bytes_count(CAPACITY, BITS_PER_ELEMENT)],
    len: u16, // Number of u6 elements currently stored. Max CAPACITY.
}

impl CompressedFixedVec {
    // Removed const N
    // No where clause needed here anymore
    /// Gets the 6-bit value at `idx`. Assumes `idx < self.len`.
    fn internal_get(&self, idx: usize) -> Option<u8> {
        // This check is technically redundant if callers ensure idx < self.len,
        // but good for safety if called directly.
        if idx >= self.len() {
            return None;
        }

        let start_bit_pos = idx * BITS_PER_ELEMENT;
        let byte_idx = start_bit_pos / 8;
        let bit_offset_in_byte = start_bit_pos % 8;

        // Create a 16-bit window to read bits that might span two bytes.
        // Max BITS_PER_ELEMENT (6) + max bit_offset_in_byte (7) = 13 bits. Fits in u16.
        let mut window = self.data[byte_idx] as u16;

        let next_byte_idx = byte_idx + 1;
        // Check if the element spans into the next byte AND that next byte is within our data array.
        if (bit_offset_in_byte + BITS_PER_ELEMENT > 8)
            && (next_byte_idx < packed_bytes_count(CAPACITY, BITS_PER_ELEMENT))
        {
            // Use CAPACITY
            window |= (self.data[next_byte_idx] as u16) << 8;
        }

        let value = (window >> bit_offset_in_byte) & MAX_VALUE_U6 as u16;
        Some(value as u8)
    }

    /// Sets the 6-bit value at `idx`. Assumes `idx` is a valid position to write (e.g. `idx < CAPACITY`).
    fn internal_set(&mut self, idx: usize, value: u8) -> Result<(), &'static str> {
        if value > MAX_VALUE_U6 {
            return Err("Value too large for u6");
        }
        // Ensure idx is within the capacity of the data array, not just current len.
        // This check is implicitly handled by array access bounds if idx is too large for packed_bytes_count(CAPACITY, ...).
        // However, a direct check against CAPACITY is clearer for the logic.
        if idx >= CAPACITY {
            return Err("Index out of capacity bounds");
        }

        let start_bit_pos = idx * BITS_PER_ELEMENT;
        let byte_idx = start_bit_pos / 8;
        let bit_offset_in_byte = start_bit_pos % 8;

        let val_u16 = value as u16;
        let value_mask_u16 = MAX_VALUE_U6 as u16;

        let mut window = self.data[byte_idx] as u16;
        let next_byte_idx = byte_idx + 1;
        let spans_to_next_byte = bit_offset_in_byte + BITS_PER_ELEMENT > 8;
        let next_byte_exists_in_data =
            next_byte_idx < packed_bytes_count(CAPACITY, BITS_PER_ELEMENT); // Use CAPACITY

        if spans_to_next_byte && next_byte_exists_in_data {
            window |= (self.data[next_byte_idx] as u16) << 8;
        }

        window &= !(value_mask_u16 << bit_offset_in_byte); // Clear bits
        window |= val_u16 << bit_offset_in_byte; // Set new bits

        self.data[byte_idx] = window as u8;
        if spans_to_next_byte && next_byte_exists_in_data {
            self.data[next_byte_idx] = (window >> 8) as u8;
        }
        Ok(())
    }

    /// Creates a new, empty `CompressedFixedVec`.
    pub fn new() -> Self {
        Self {
            data: [0u8; packed_bytes_count(CAPACITY, BITS_PER_ELEMENT)], // Use CAPACITY
            len: 0,
        }
    }

    /// Returns the number of elements in the vector.
    pub fn len(&self) -> usize {
        self.len as usize
    }

    /// Returns `true` if the vector contains no elements.
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Returns the total number of elements the vector can hold.
    pub fn capacity(&self) -> usize {
        CAPACITY // Use CAPACITY
    }

    /// Returns `true` if the vector is full.
    pub fn is_full(&self) -> bool {
        self.len as usize == CAPACITY // Use CAPACITY
    }

    /// Appends an element to the back of the collection.
    pub fn push(&mut self, value: u8) -> Result<(), &'static str> {
        if self.is_full() {
            return Err("CompressedFixedVec is full");
        }
        // internal_set checks value range.
        self.internal_set(self.len(), value)?; // Write at current len, then increment
        self.len += 1;
        Ok(())
    }

    /// Removes the last element from a vector and returns it, or `None` if it is empty.
    pub fn pop(&mut self) -> Option<u8> {
        if self.is_empty() {
            None
        } else {
            let last_idx = self.len() - 1; // Index of the element to be popped
                                           // Retrieve the value using internal_get.
                                           // last_idx is guaranteed to be < self.len() (original length) here,
                                           // so internal_get's own check `idx >= self.len()` will pass correctly.
            let value = self.internal_get(last_idx).unwrap();

            self.len -= 1; // Now, decrement the length

            // Clear bits of the popped element at its original index.
            self.internal_set(last_idx, 0).unwrap();
            Some(value)
        }
    }

    /// Returns the element at `index`, or `None` if out of bounds.
    pub fn get(&self, index: usize) -> Option<u8> {
        if index < self.len() {
            self.internal_get(index)
        } else {
            None
        }
    }

    /// Sets the element at `index` to `value`.
    /// Returns error if index is out of bounds or value is too large.
    pub fn set(&mut self, index: usize, value: u8) -> Result<(), &'static str> {
        if index >= self.len() {
            return Err("Index out of bounds");
        }
        self.internal_set(index, value)
    }

    /// Returns a mutable proxy `U6RefMut` for the element at `index`.
    pub fn get_mut(&mut self, index: usize) -> Option<U6RefMut<'_>> {
        // Removed N
        if index < self.len() {
            Some(U6RefMut {
                vec: self,
                idx: index,
            })
        } else {
            None
        }
    }

    /// Clears the vector, removing all values.
    pub fn clear(&mut self) {
        let bytes_to_clear = packed_bytes_count(self.len(), BITS_PER_ELEMENT);
        if bytes_to_clear > 0 {
            // Avoid slicing on N=0 or empty vec.
            self.data[0..bytes_to_clear].fill(0);
        }
        self.len = 0;
    }

    /// Fills the entire vector with `value`. All previous elements are overwritten.
    pub fn fill(&mut self, value: u8) -> Result<(), &'static str> {
        if value > MAX_VALUE_U6 {
            return Err("Value too large for u6");
        }
        if CAPACITY == 0 {
            // Use CAPACITY
            self.len = 0;
            return Ok(());
        }

        if value == 0 {
            // Optimized path for zero-fill
            let total_bytes = packed_bytes_count(CAPACITY, BITS_PER_ELEMENT); // Use CAPACITY
            if total_bytes > 0 {
                self.data[0..total_bytes].fill(0);
            }
        } else {
            for i in 0..CAPACITY {
                self.internal_set(i, value)?;
            } // Use CAPACITY
        }
        self.len = CAPACITY as u16; // Use CAPACITY
        Ok(())
    }

    /// Returns an iterator over the elements.
    pub fn iter(&self) -> CompressedVecIter<'_> {
        // Removed N
        CompressedVecIter {
            vec: self,
            current: 0,
        }
    }

    /// Returns the first element, or `None` if empty.
    pub fn first(&self) -> Option<u8> {
        if self.is_empty() {
            None
        } else {
            self.internal_get(0)
        }
    }

    /// Returns the last element, or `None` if empty.
    pub fn last(&self) -> Option<u8> {
        if self.is_empty() {
            None
        } else {
            self.internal_get(self.len() - 1)
        }
    }

    // Methods like remove, insert, retain, extend can be implemented similarly to FixedVec,
    // using internal_get/set. They will be less efficient due to per-element bit manipulation.
    // Example: remove
    pub fn remove(&mut self, index: usize) -> Option<u8> {
        if index >= self.len() {
            return None;
        }
        let removed_val = self.get(index).unwrap(); // Safe due to check
        for i in index..(self.len() - 1) {
            let next_val = self.get(i + 1).unwrap();
            self.internal_set(i, next_val).unwrap();
        }
        self.len -= 1;
        self.internal_set(self.len(), 0).unwrap(); // Clear the last (now unused) spot
        Some(removed_val)
    }
}

impl Default for CompressedFixedVec {
    // Removed const N and where clause
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Debug for CompressedFixedVec {
    // Removed const N and where clause
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Similar to FixedVec's debug, but shows actual values
        write!(
            f,
            "CompressedFixedVec<{}>({}/{}) [",
            CAPACITY,
            self.len(),
            CAPACITY
        )?; // Use CAPACITY
        for i in 0..self.len() {
            if i > 0 {
                write!(f, ", ")?;
            }
            // self.get(i) is safe to unwrap here
            fmt::Debug::fmt(&self.get(i).unwrap(), f)?;
        }
        write!(f, "]")
    }
}

impl PartialEq for CompressedFixedVec {
    // Removed const N and where clause
    fn eq(&self, other: &Self) -> bool {
        if self.len != other.len {
            return false;
        }
        self.iter().zip(other.iter()).all(|(a, b)| a == b)
    }
}
impl Eq for CompressedFixedVec // Removed const N and where clause
{
}

// Iterator
pub struct CompressedVecIter<'a> {
    // Removed const N
    vec: &'a CompressedFixedVec, // Removed <N>
    current: usize,
}

impl Iterator for CompressedVecIter<'_> {
    // Removed const N
    type Item = u8;
    fn next(&mut self) -> Option<Self::Item> {
        if self.current < self.vec.len() {
            let value = self.vec.internal_get(self.current).unwrap(); // Safe
            self.current += 1;
            Some(value)
        } else {
            None
        }
    }
}
impl ExactSizeIterator for CompressedVecIter<'_> {
    // Removed const N
    fn len(&self) -> usize {
        self.vec.len() - self.current
    }
}

// IntoIterator
pub struct CompressedVecIntoIter {
    // Removed const N
    vec: CompressedFixedVec, // Removed <N>
    current: usize,
}

impl Iterator for CompressedVecIntoIter {
    // Removed const N
    type Item = u8;
    fn next(&mut self) -> Option<Self::Item> {
        if self.current < self.vec.len() {
            let value = self.vec.internal_get(self.current).unwrap(); // Safe
            self.current += 1;
            Some(value)
        } else {
            None
        }
    }
}
impl ExactSizeIterator for CompressedVecIntoIter {
    // Removed const N
    fn len(&self) -> usize {
        self.vec.len() - self.current
    }
}

impl IntoIterator for CompressedFixedVec {
    // Removed const N and where clause
    type Item = u8;
    type IntoIter = CompressedVecIntoIter; // Removed <N>
    fn into_iter(self) -> Self::IntoIter {
        CompressedVecIntoIter {
            vec: self,
            current: 0,
        }
    }
}

// Borsh (for Anchor)
impl AnchorSerialize for CompressedFixedVec {
    // Removed const N and where clause
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> Result<(), std::io::Error> {
        borsh::BorshSerialize::serialize(&self.len, writer)?;
        let num_data_bytes = packed_bytes_count(self.len(), BITS_PER_ELEMENT);
        if num_data_bytes > 0 {
            writer.write_all(&self.data[..num_data_bytes])?;
        }
        Ok(())
    }
}

impl AnchorDeserialize for CompressedFixedVec {
    // Removed const N and where clause
    fn deserialize(buf: &mut &[u8]) -> Result<Self, std::io::Error> {
        let len: u16 = borsh::BorshDeserialize::deserialize(buf)?;

        if len as usize > CAPACITY {
            // Use CAPACITY
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Deserialized length exceeds capacity",
            ));
        }

        let mut data_arr = [0u8; packed_bytes_count(CAPACITY, BITS_PER_ELEMENT)]; // Use CAPACITY
        let num_data_bytes_to_read = packed_bytes_count(len as usize, BITS_PER_ELEMENT);

        if num_data_bytes_to_read > 0 {
            if buf.len() < num_data_bytes_to_read {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "Buffer too small for deserialization",
                ));
            }
            let (src_data, rest_buf) = buf.split_at(num_data_bytes_to_read);
            data_arr[..num_data_bytes_to_read].copy_from_slice(src_data);
            *buf = rest_buf;
        }
        Ok(CompressedFixedVec {
            data: data_arr,
            len,
        })
    }

    fn deserialize_reader<R: Read>(reader: &mut R) -> Result<Self, std::io::Error>
    where
        Self: Sized,
    {
        // Default BorshDeserialize implementation pattern
        let mut buffer = Vec::new();
        reader.read_to_end(&mut buffer)?;
        let mut slice = buffer.as_slice();
        // Call the BorshDeserialize version explicitly
        <Self as borsh::BorshDeserialize>::deserialize(&mut slice)
    }
}

// Serde
impl Serialize for CompressedFixedVec {
    // Removed const N and where clause
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut seq = serializer.serialize_seq(Some(self.len()))?;
        for val in self.iter() {
            // Use existing iter()
            seq.serialize_element(&val)?;
        }
        seq.end()
    }
}

impl<'de> Deserialize<'de> for CompressedFixedVec {
    // Removed const N and where clause
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct VecVisitor; // Removed const V_N and where clause

        impl<'de> Visitor<'de> for VecVisitor {
            // Removed const V_N and where clause
            type Value = CompressedFixedVec; // Removed <V_N>

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str(&format!(
                    "a sequence of u8 numbers (0-{}) of at most {} elements",
                    MAX_VALUE_U6, CAPACITY
                )) // Use CAPACITY
            }

            fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
            where
                A: SeqAccess<'de>,
            {
                let mut vec = CompressedFixedVec::new();
                // Pre-allocate if size_hint is available, though for FixedVec it's fixed capacity.
                // let capacity = seq.size_hint().unwrap_or(0).min(CAPACITY);

                while let Some(value) = seq.next_element::<u8>()? {
                    // Store the result of push to check the specific error
                    let push_result = vec.push(value);
                    if push_result.is_err() {
                        let push_err_msg = push_result.err().unwrap_or("Push failed"); // Safe unwrap due to is_err check
                        if push_err_msg == "CompressedFixedVec is full" {
                            return Err(serde::de::Error::custom(format_args!(
                                "Too many elements for CompressedFixedVec capacity {}",
                                CAPACITY // Use CAPACITY
                            )));
                        } else {
                            // Likely "Value too large for u6" or other internal_set error
                            return Err(serde::de::Error::custom(format_args!(
                                "Invalid u6 value or other push error: {} (value: {})",
                                push_err_msg, value
                            )));
                        }
                    }
                }
                Ok(vec)
            }
        }
        deserializer.deserialize_seq(VecVisitor) // Removed <N>
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    // Note: Tests will need significant adaptation as CAPACITY is now 256.
    // Many tests were written for small N.

    #[test]
    fn test_new_empty_full_capacity() {
        let vec: CompressedFixedVec = CompressedFixedVec::new();
        assert_eq!(vec.len(), 0);
        assert!(vec.is_empty());
        assert!(!vec.is_full()); // Assuming CAPACITY (256) > 0
        assert_eq!(vec.capacity(), CAPACITY); // Check against global CAPACITY
    }

    #[test]
    fn test_push_get_pop() {
        let mut vec: CompressedFixedVec = CompressedFixedVec::new();
        vec.push(10).unwrap();
        vec.push(20).unwrap();
        vec.push(30).unwrap();

        assert_eq!(vec.len(), 3);
        assert_eq!(vec.get(0), Some(10));
        assert_eq!(vec.get(1), Some(20));
        assert_eq!(vec.get(2), Some(30));
        assert_eq!(vec.get(3), None);

        // Internal data check for 3 elements (requires 3 bytes: 3*6=18 bits)
        // This part of the test is highly dependent on the old N=4.
        // For N=CAPACITY, data array is much larger.
        // We can still check the packed bytes for the current length.
        let current_packed_size = packed_bytes_count(3, BITS_PER_ELEMENT);
        let mut expected_data_segment = vec![0u8; current_packed_size];
        // Manually calculate expected bytes for [10, 20, 30]
        // 10 (001010), 20 (010100), 30 (011110)
        // Byte 0: 00001010 (10) (bits 0-5 from 10, bits 6-7 from 20)
        //   Correct: val0_5..0 | val1_1..0 << 6
        //   val0 = 10 (001010)
        //   val1 = 20 (010100)
        //   data[0] = (20 & 0b11) << 6 | 10 = (0b00 << 6) | 0b001010 = 0b00001010 = 0
        // Byte 1: 01111001 (229) (bits 0-3 from 20, bits 4-7 from 30)
        //   Correct: val1_5..2 >> 2 | val2_3..0 << 4
        //   data[1] = (30 & 0b1111) << 4 | (20 >> 2) = (0b1110 << 4) | 0b0101 = 0b11100101 = 229
        // Byte 2: 00000001 (1) (bits 0-1 from 30)
        //   Correct: val2_5..4 >> 4
        //   data[2] = (30 >> 4) = 0b01 = 1
        expected_data_segment[0] = 10;
        expected_data_segment[1] = 229;
        expected_data_segment[2] = 1;
        assert_eq!(vec.data[0..current_packed_size], expected_data_segment);

        assert_eq!(vec.pop(), Some(30));
        assert_eq!(vec.len(), 2);
        assert_eq!(vec.get(2), None);
        // After pop, data for the 3rd element (index 2) should be cleared.
        // internal_set(2,0) affects byte_idx=1 and byte_idx=2
        // data[1] bits 4-7 zeroed. data[2] bits 0-1 zeroed.
        // Original data[1] = 0b11100101. Mask for bits 4-7 is 0xF0.
        // data[1] becomes 0b00000101 = 5.
        // Original data[2] = 0b00000001. Mask for bits 0-1 is 0x03.
        // data[2] becomes 0b00000000 = 0.
        expected_data_segment[0] = 10;
        expected_data_segment[1] = 5;
        expected_data_segment[2] = 0;
        assert_eq!(vec.data[0..current_packed_size], expected_data_segment);

        assert_eq!(vec.pop(), Some(20));
        assert_eq!(vec.pop(), Some(10));
        assert_eq!(vec.pop(), None);
        assert!(vec.is_empty());
    }

    #[test]
    fn test_fill_clear() {
        let mut vec: CompressedFixedVec = CompressedFixedVec::new(); // N is fixed
                                                                     // This test used N=5. For N=CAPACITY, the data array will be much larger.
                                                                     // The logic of fill and clear should still hold.
        vec.fill(42).unwrap();
        assert!(vec.is_full());
        assert_eq!(vec.len(), CAPACITY);
        for i in 0..CAPACITY {
            assert_eq!(vec.get(i), Some(42));
        }

        // Data check for fill(42) is complex for large CAPACITY.
        // We can check a small segment if needed, or trust the loop.
        // For 42 (101010), the byte pattern is AA AA AA 2A for 5 elements.
        // This pattern will repeat.
        // Example: first few bytes for CAPACITY >= 4
        if CAPACITY >= 4 {
            // packed_bytes_count(4,6) = 3 bytes
            // packed_bytes_count(5,6) = 4 bytes
            assert_eq!(vec.data[0], 0xAA); // 10101010
            assert_eq!(vec.data[1], 0xAA); // 10101010
            assert_eq!(vec.data[2], 0xAA); // 10101010
                                           // For 5th element, it starts at bit 24 (byte 3, bit 0)
                                           // data[3] should contain bits of 5th '42'
                                           // 42 = 101010. Bits are ... [42] [42] [42] [42] [42]
                                           // ... 101010 | 101010 | 101010 | 101010 | 101010
                                           // byte0: 10101010
                                           // byte1: 10101010
                                           // byte2: 10101010
                                           // byte3: xx101010 (bits from 4th and 5th element)
                                           // 4th element (idx 3) starts at bit 18. byte_idx=2, bit_offset=2.
                                           // self.data[2] = (val >> 2) | (val << 6) & 0xC0 ...
                                           // 5th element (idx 4) starts at bit 24. byte_idx=3, bit_offset=0.
                                           // self.data[3] = (val >> 0) & 0x3F | ...
                                           // So data[3] should start with 101010... = 0x2A if higher bits are 0.
            if packed_bytes_count(CAPACITY, BITS_PER_ELEMENT) > 3 {
                assert_eq!(vec.data[3] & 0x3F, 0x2A); // Check lower 6 bits
            }
        }

        vec.clear();
        assert!(vec.is_empty());
        assert_eq!(vec.len(), 0);
        // Check that the cleared portion of data is zero.
        let bytes_cleared_for_cap = packed_bytes_count(CAPACITY, BITS_PER_ELEMENT);
        assert!(vec.data[0..bytes_cleared_for_cap].iter().all(|&x| x == 0));
    }

    #[test]
    fn test_set_get_mut() {
        let mut vec: CompressedFixedVec = CompressedFixedVec::new(); // N is fixed
        vec.push(1).unwrap();
        vec.push(2).unwrap();
        vec.push(3).unwrap();

        vec.set(1, 15).unwrap();
        assert_eq!(vec.get(1), Some(15));

        vec.get_mut(0).unwrap().set(10);
        assert_eq!(vec.get(0), Some(10));

        assert!(vec.set(CAPACITY, 5).is_err()); // Index CAPACITY is OOB for len, but also for setting if len was CAPACITY
        assert!(vec.set(3, 5).is_err()); // vec.len is 3, so index 3 is OOB for current length

        assert!(vec.set(0, 64).is_err());
        assert!(vec.push(64).is_err());
    }

    #[test]
    #[should_panic(expected = "Value out of u6 range")]
    fn test_get_mut_set_panic_value_range() {
        let mut vec: CompressedFixedVec = CompressedFixedVec::new(); // N is fixed
        vec.push(0).unwrap();
        vec.get_mut(0).unwrap().set(64);
    }

    #[test]
    fn test_iter() {
        let mut vec: CompressedFixedVec = CompressedFixedVec::new(); // N is fixed
        vec.push(5).unwrap();
        vec.push(15).unwrap();
        vec.push(25).unwrap();

        let mut iter = vec.iter();
        assert_eq!(iter.next(), Some(5));
        assert_eq!(iter.next(), Some(15));
        assert_eq!(iter.next(), Some(25));
        assert_eq!(iter.next(), None);
    }

    #[test]
    fn test_into_iter() {
        let mut vec: CompressedFixedVec = CompressedFixedVec::new(); // N is fixed
        vec.push(1).unwrap();
        vec.push(2).unwrap();
        vec.push(3).unwrap();

        let collected: Vec<u8> = vec.into_iter().collect();
        assert_eq!(collected, vec![1, 2, 3]);
    }
}
