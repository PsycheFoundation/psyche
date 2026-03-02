use crate as psyche_core;
use anchor_lang::{AnchorDeserialize, AnchorSerialize, prelude::borsh};
use bytemuck::Zeroable;
use serde::{Deserialize, Serialize};
use std::ops::{Deref, DerefMut, Range, RangeFrom, RangeFull, RangeTo};
use ts_rs::TS;

#[derive(Clone, Copy, Zeroable, AnchorSerialize, AnchorDeserialize, PartialEq, TS)]
#[ts(type = "Array<T>", bound = "T: TS")]
#[repr(C)]
pub struct FixedVec<T, const N: usize> {
    data: [T; N],
    len: u64,
}

impl<T: Default + Copy, const N: usize> FixedVec<T, N> {
    pub fn new() -> Self {
        Self {
            data: [T::default(); N],
            len: 0,
        }
    }

    pub fn len(&self) -> usize {
        self.len as usize
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn is_full(&self) -> bool {
        self.len as usize == N
    }

    pub fn capacity(&self) -> usize {
        N
    }

    pub fn push(&mut self, value: T) -> Result<(), &'static str> {
        if (self.len as usize) < N {
            self.data[self.len as usize] = value;
            self.len += 1;
            Ok(())
        } else {
            Err("FixedVec is full")
        }
    }

    pub fn pop(&mut self) -> Option<T> {
        if self.len > 0 {
            self.len -= 1;
            Some(std::mem::take(&mut self.data[self.len as usize]))
        } else {
            None
        }
    }

    pub fn get(&self, index: usize) -> Option<&T> {
        if index < self.len as usize {
            Some(&self.data[index])
        } else {
            None
        }
    }

    pub fn get_mut(&mut self, index: usize) -> Option<&mut T> {
        if index < self.len as usize {
            Some(&mut self.data[index])
        } else {
            None
        }
    }

    pub fn clear(&mut self) {
        for i in 0..self.len as usize {
            self.data[i] = T::default();
        }
        self.len = 0;
    }

    pub fn iter(&self) -> impl DoubleEndedIterator<Item = &T> {
        self.data[0..self.len as usize].iter()
    }

    pub fn iter_mut(&mut self) -> impl DoubleEndedIterator<Item = &mut T> {
        self.data[0..self.len as usize].iter_mut()
    }

    pub fn extend<I: IntoIterator<Item = T>>(&mut self, iter: I) -> Result<(), &'static str> {
        for item in iter {
            self.push(item)?;
        }
        Ok(())
    }

    pub fn first(&self) -> Option<&T> {
        if self.len > 0 {
            Some(&self.data[0])
        } else {
            None
        }
    }

    pub fn last(&self) -> Option<&T> {
        if self.len > 0 {
            Some(&self.data[self.len as usize - 1])
        } else {
            None
        }
    }

    pub fn remove(&mut self, index: usize) -> Option<T> {
        if index >= self.len as usize {
            return None;
        }

        let item = Some(self.data[index]);

        let last_pos = self.len as usize - 1;
        for i in index..last_pos {
            self.data[i] = self.data[i + 1];
        }

        self.data[last_pos] = T::default();
        self.len -= 1;
        item
    }

    pub fn insert(&mut self, index: usize, element: T) -> Result<(), &'static str> {
        if self.len as usize >= N {
            return Err("FixedVec is full");
        }
        if index > self.len as usize {
            return Err("Index out of bounds");
        }

        for i in (index..self.len as usize).rev() {
            self.data[i + 1] = self.data[i];
        }

        self.data[index] = element;
        self.len += 1;
        Ok(())
    }

    pub fn retain<F>(&mut self, mut f: F)
    where
        F: FnMut(&T) -> bool,
    {
        let mut read = 0;
        let mut write = 0;

        while read < self.len as usize {
            if f(&self.data[read]) {
                if read != write {
                    self.data[write] = self.data[read];
                }
                write += 1;
            }
            read += 1;
        }

        // zero-out the rest of the positions which are now unused
        for i in write..self.len as usize {
            self.data[i] = T::default();
        }
        self.len = write as u64;
    }
}

impl<T: std::fmt::Debug, const N: usize> std::fmt::Debug for FixedVec<T, N> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "FixedVec<{}> {}x(", N, self.len)?;
        f.debug_list()
            .entries(self.data[0..self.len as usize].iter())
            .finish()?;
        write!(f, ")")?;
        Ok(())
    }
}

impl<T: Default + Copy, const N: usize> std::ops::Index<usize> for FixedVec<T, N> {
    type Output = T;

    fn index(&self, index: usize) -> &Self::Output {
        self.get(index).expect("Index out of bounds")
    }
}

impl<T: Default + Copy, const N: usize> std::ops::IndexMut<usize> for FixedVec<T, N> {
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        self.get_mut(index).expect("Index out of bounds")
    }
}

impl<T: Default + Copy, const N: usize> Default for FixedVec<T, N> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Default + Copy, const N: usize> std::ops::Index<Range<usize>> for FixedVec<T, N> {
    type Output = [T];

    fn index(&self, range: Range<usize>) -> &Self::Output {
        if range.start > range.end || range.end > self.len as usize {
            panic!("Index out of bounds");
        }
        &self.data[range]
    }
}

impl<T: Default + Copy, const N: usize> std::ops::Index<RangeFull> for FixedVec<T, N> {
    type Output = [T];

    fn index(&self, _: RangeFull) -> &Self::Output {
        &self.data[..self.len as usize]
    }
}

impl<T: Default + Copy, const N: usize> std::ops::Index<RangeFrom<usize>> for FixedVec<T, N> {
    type Output = [T];

    fn index(&self, range: RangeFrom<usize>) -> &Self::Output {
        if range.start > self.len as usize {
            panic!("Index out of bounds");
        }
        &self.data[range.start..self.len as usize]
    }
}

impl<T: Default + Copy, const N: usize> std::ops::Index<RangeTo<usize>> for FixedVec<T, N> {
    type Output = [T];

    fn index(&self, range: RangeTo<usize>) -> &Self::Output {
        if range.end > self.len as usize {
            panic!("Index out of bounds");
        }
        &self.data[..range.end]
    }
}

impl<T: Default + Copy, const N: usize> std::ops::IndexMut<Range<usize>> for FixedVec<T, N> {
    fn index_mut(&mut self, range: Range<usize>) -> &mut Self::Output {
        if range.start > range.end || range.end > self.len as usize {
            panic!("Index out of bounds");
        }
        &mut self.data[range]
    }
}

impl<T: Default + Copy, const N: usize> std::ops::IndexMut<RangeFull> for FixedVec<T, N> {
    fn index_mut(&mut self, _: RangeFull) -> &mut Self::Output {
        &mut self.data[..self.len as usize]
    }
}

impl<T: Default + Copy, const N: usize> std::ops::IndexMut<RangeFrom<usize>> for FixedVec<T, N> {
    fn index_mut(&mut self, range: RangeFrom<usize>) -> &mut Self::Output {
        if range.start > self.len as usize {
            panic!("Index out of bounds");
        }
        &mut self.data[range.start..self.len as usize]
    }
}

impl<T: Default + Copy, const N: usize> std::ops::IndexMut<RangeTo<usize>> for FixedVec<T, N> {
    fn index_mut(&mut self, range: RangeTo<usize>) -> &mut Self::Output {
        if range.end > self.len as usize {
            panic!("Index out of bounds");
        }
        &mut self.data[..range.end]
    }
}

impl<T: Default + Copy, const N: usize> Deref for FixedVec<T, N> {
    type Target = [T];

    fn deref(&self) -> &Self::Target {
        &self.data[..self.len as usize]
    }
}

impl<T: Default + Copy, const N: usize> DerefMut for FixedVec<T, N> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.data[..self.len as usize]
    }
}

impl<T: Default + Copy, const N: usize> FixedVec<T, N> {
    pub fn try_from_iter<I: IntoIterator<Item = T>>(iter: I) -> Result<Self, &'static str> {
        let mut vec = Self::new();
        vec.extend(iter)?;
        Ok(vec)
    }
}

impl<T: Default + Copy, const N: usize> FromIterator<T> for FixedVec<T, N> {
    fn from_iter<I: IntoIterator<Item = T>>(iter: I) -> Self {
        Self::try_from_iter(iter).expect("Iterator too long for FixedVec capacity")
    }
}

impl<T: Default + Copy, const N: usize> TryFrom<&[T]> for FixedVec<T, N> {
    type Error = &'static str;

    fn try_from(slice: &[T]) -> Result<Self, Self::Error> {
        Self::try_from_iter(slice.iter().copied())
    }
}

impl<T: Default + Copy, const N: usize, const M: usize> TryFrom<[T; M]> for FixedVec<T, N> {
    type Error = &'static str;

    fn try_from(array: [T; M]) -> Result<Self, Self::Error> {
        Self::try_from_iter(array)
    }
}

impl<T: Serialize + Default + Copy, const N: usize> Serialize for FixedVec<T, N> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let vec: Vec<_> = self.iter().collect();
        vec.serialize(serializer)
    }
}

impl<'de, T: Deserialize<'de> + Default + Copy, const N: usize> Deserialize<'de>
    for FixedVec<T, N>
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let vec = Vec::<T>::deserialize(deserializer)?;

        let mut fixed_vec = FixedVec::new();
        for item in vec {
            fixed_vec.push(item).map_err(serde::de::Error::custom)?;
        }

        Ok(fixed_vec)
    }
}

impl<T, const N: usize> IntoIterator for FixedVec<T, N> {
    type Item = T;
    type IntoIter = std::iter::Take<std::array::IntoIter<T, N>>;
    fn into_iter(self) -> Self::IntoIter {
        self.data.into_iter().take(self.len as usize)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_initial_state() {
        let vec: FixedVec<u32, 6> = FixedVec::new();
        assert_eq!(vec.len(), 0);
        assert_eq!(vec.capacity(), 6);
        assert!(vec.is_empty());
        assert!(!vec.is_full());
        assert_eq!(vec.get(0), None);
        for i in 0..vec.capacity() {
            assert_eq!(vec.get(i), None);
            assert_eq!(vec.data[i], 0u32);
        }
    }

    #[test]
    fn test_push_and_access() {
        let mut vec: FixedVec<u32, 6> = FixedVec::new();
        vec.push(1).unwrap();
        vec.push(2).unwrap();
        vec.push(3).unwrap();

        assert_eq!(vec.len(), 3);
        assert_eq!(vec.capacity(), 6);
        assert!(!vec.is_empty());
        assert!(!vec.is_full());
        assert_eq!(vec[0], 1);
        assert_eq!(vec[1], 2);
        assert_eq!(vec[2], 3);
        assert_eq!(vec.get(3), None);
    }

    #[test]
    fn test_pop() {
        let mut vec: FixedVec<u32, 6> = FixedVec::new();
        vec.push(1).unwrap();
        vec.push(2).unwrap();
        vec.push(3).unwrap();

        assert_eq!(vec.pop(), Some(3));
        assert_eq!(vec.len(), 2);
        assert_eq!(vec[0], 1);
        assert_eq!(vec[1], 2);
        assert_eq!(vec.get(2), None);
    }

    #[test]
    fn test_insert_and_remove() {
        let mut vec: FixedVec<u32, 6> = FixedVec::new();
        vec.push(1).unwrap();
        vec.push(2).unwrap();
        vec.push(3).unwrap();

        // Insert
        vec.insert(1, 9).unwrap();
        assert_eq!(vec.len(), 4);
        assert_eq!(vec[0], 1);
        assert_eq!(vec[1], 9);
        assert_eq!(vec[2], 2);
        assert_eq!(vec[3], 3);

        // Remove
        vec.remove(1);
        assert_eq!(vec.len(), 3);
        assert_eq!(vec[0], 1);
        assert_eq!(vec[1], 2);
        assert_eq!(vec[2], 3);
    }

    #[test]
    fn test_full_vec_behavior() {
        let mut vec: FixedVec<u32, 6> = FixedVec::new();
        vec.extend([1, 2, 3, 4, 5, 6]).unwrap();

        assert!(vec.is_full());
        assert_eq!(vec.len(), 6);

        // Attempt to push to a full vec
        let res = vec.push(7);
        assert_eq!(res, Err("FixedVec is full"));

        // Attempt to insert into a full vec
        let res = vec.insert(1, 9);
        assert_eq!(res, Err("FixedVec is full"));
    }

    #[test]
    fn test_retain() {
        let mut vec: FixedVec<u32, 6> = FixedVec::new();
        vec.extend([1, 2, 3, 4, 5, 6]).unwrap();

        // Retain only even numbers
        vec.retain(|x| x % 2 == 0);
        assert_eq!(vec.len(), 3);
        assert_eq!(vec[0], 2);
        assert_eq!(vec[1], 4);
        assert_eq!(vec[2], 6);
    }

    #[test]
    fn test_clear() {
        let mut vec: FixedVec<u32, 6> = FixedVec::new();
        vec.extend([1, 2, 3, 4, 5, 6]).unwrap();
        assert!(vec.is_full());

        vec.clear();
        assert!(vec.is_empty());
        assert!(!vec.is_full());
        assert_eq!(vec.len(), 0);

        for i in 0..vec.capacity() {
            assert_eq!(vec.get(i), None);
            assert_eq!(vec.data[i], 0u32);
        }
    }

    #[test]
    fn test_clear_empty_vec() {
        let mut vec: FixedVec<u32, 6> = FixedVec::new();
        assert!(vec.is_empty());

        vec.clear();
        assert!(vec.is_empty());
        assert!(!vec.is_full());

        for i in 0..vec.capacity() {
            assert_eq!(vec.get(i), None);
            assert_eq!(vec.data[i], 0u32);
        }
    }
}
