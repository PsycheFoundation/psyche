
# Psyche Coordinator - Breaking Changes

## Abstract

Once a Solana account is initialized and stores a data structure, its binary layout becomes locked in, meaning any update to the smart contract account's data layout must either:

- Preserve ABI compatibility
- Perform explicit migrations

In practice, it means that any update to the Coordinator's data structure needs to abide by this constraints.

Anchor derives layout using `#[account]` and `AnchorSerialize`/`AnchorDeserialize`. It's critical not to change enum variants, reorder fields, or change types without bumping version fields and writing explicit migrations.

This is because if the program's logic is being upgraded but the on-chain account still retains the old memory layout that was relied upon before the smart contract was upgraded, the new smart contract version will fail to read the old account's state properly, leading to runtime errors and vulnerabilities.

## Mitigations

There are a few types of potential avenues to mitigate the problem, each can be applied in different situations

### 1) Architechtural changes

A few code logic changes can help make future breaking changes more forgiving.

#### A) Use PDAs for modular storage

Due to the nature of serialization/deserialization where all information is stored sequentially in a byte array: the bigger the datastructure is, the more likely it is to introduce a breaking change.

It then makes a lot of sense to split the program's state into multiple PDAs to avoid a large monolithic state, this would allow for easier migrations of smaller PDAs. Different PDAs could use different mitigation strategies independently, depending on the specific situation.

It helps upgrade and migrate individual chunks of data and also helps avoiding the 10KB max size per account "soft limit".

#### B) Add data-structure versionning

Adding a versionning system to the data structures enables conditional migration logic. This can be done through either:

- an `Enum` of which each case version (most comprehensive and complex)
- a `version` field on the data structure (most simple)

Also don't forget to use `#[repr(C)]` to ensure the predictability of the memory layout as by default the `#[repr(Rust)]` has undefined behaviour and its memory layout is left to the responsibility of the compiler's optimizer implementation (relevant for bytemuck serialized accounts).

### 2) Backward compatible changes

In some cases, it is possible to make changes to the memory layout without requiring any migrations but it requires planning in advance.

```rust
// Before
#[account]
#[repr(C, packed)]
pub struct MyAccountV1 {
    pub version: u8,
    pub my_field1: u64,
    pub _reserved: [u8, 256], // Zeroed out memory for future use
}
// After
#[account]
#[repr(C, packed)]
pub struct MyAccountV2 {
    pub version: u8,
    pub my_field1: u64,
    pub my_field2: u32, // my_field2 is initialized to zero on smart contract upgrade
    pub _reserved: [u8, 252], // Adjusted size, 4 bytes now used by my_field2
}
```

In those cases, some planning and careful changes can achieve changes that require no migrations.

### 3) Explicit Migrations

