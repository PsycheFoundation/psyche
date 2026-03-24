use serde::{Deserialize, Deserializer, Serialize, Serializer};

pub fn serde_serialize_array_as_vec<S, T: Serialize + Clone>(
    array: &[T],
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    array.to_vec().serialize(serializer)
}

pub fn serde_deserialize_vec_to_array<'de, D, T, const N: usize>(
    deserializer: D,
) -> Result<[T; N], D::Error>
where
    D: Deserializer<'de>,
    T: Default + Copy + Deserialize<'de>,
{
    let vec = Vec::<T>::deserialize(deserializer)?;
    let mut arr = [T::default(); N];
    let len = std::cmp::min(vec.len(), N);
    arr[..len].copy_from_slice(&vec[..len]);
    Ok(arr)
}
