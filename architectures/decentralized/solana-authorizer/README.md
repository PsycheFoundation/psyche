# Psyche Solana Authorizer

This smart contract is a piece of the Psyche's onchain logic dedicated to giving permissions to specific users.

## How it works

The Authorizer smart contract manages `Authorization` PDAs.

Each `Authorization` PDA represents:

A single role (the `scope`), assigned to a single user (the `grantee`), by a specific authority (the `grantor`).

The `grantee` is then able to select a list of other keys that can act on its behalf (the `delegates`).

Conceptually an `Authorization` is made of:

```rust
// This is all the on-chain data relevant to the user
pub struct Authorization {
    pub grantor: Pubkey, // The authority granting the new permissions
    pub grantee: Pubkey, // The user receiving the new role
    pub scope: Vec<u8>, // The byte array representing the role being assigned to the grantee
    pub active: bool, // Activation flag that can be set at any time by the grantor
    pub delegates: Vec<Pubkey>, // List of delegates set by the grantee
}
// This is a simple function that can be used to check if a user has the proper permissions
impl Authorization {
    pub fn is_valid_for(
        &self,
        grantor: &Pubkey,
        grantee: &Pubkey,
        scope: &[u8],
    ) -> bool {
        if !self.active {
            return false;
        }
        if !self.grantor.eq(grantor) {
            return false;
        }
        if !self.scope.eq(scope) {
            return false;
        }
        self.grantee == Pubkey::default()
            || self.grantee.eq(grantee)
            || self.delegates.contains(grantee)
    }
}
```

The smart contract then exposes a set of instruction to manipulate those `Authorization` PDAs:

- `authorization_create`, create a new PDA
- `authorization_grantor_update`, allow the grantor to activate/deactivate the authorization
- `authorization_grantee_update`, allow the grantee to add/remove delegates
- `authorization_close` allow the grantor to remove the PDA

## Example usages

Note: We'll use the `jq` library in this example which is an open-source JSON cli tool <https://jqlang.org/>
Note: We'll also use the `solana-toolbox` from the `cargo install solana_toolbox_cli` rust crate

## Setting up an authorized user from the authority

Here is an example workflow to create and activate an authorization

```sh
# Deployed authorizer
PSYCHE_AUTHORIZER_ID="PsyAUmhpmiUouWsnJdNGFSX8vZ6rWjXjgDPHsgqPGyw"
# We need to define the SCOPE of our authorization (what it's for)
PSYCHE_AUTH_SCOPE="utf8:CoordinatorJoinRun"

# Assuming we have the authority.json keypair in the current folder
# Our authority will be our authorization GRANTOR
GRANTOR_KEYPAIR=authority.json
# Assuming we know the pubkey of the authorized user somehow
# Our authorized user will be our authorization GRANTEE
GRANTEE_PUBKEY=$(solana-keygen pubkey user.json)

# Create a new authorization and save the created PDA's address:
AUTHORIZATION_CREATE_JSON=$(\
    solana-toolbox --rpc=devnet instruction \
        $PSYCHE_AUTHORIZER_ID authorization_create \
        payer:keypair \
        grantor:$GRANTOR_KEYPAIR \
        --args=params.grantee:$GRANTEE_PUBKEY \
        --args=params.scope:$PSYCHE_AUTH_SCOPE \
        --execute
)

# Read the authorization PDA from the create instruction JSON
AUTHORIZATION_PUBKEY=$(echo $AUTHORIZATION_CREATE_JSON | jq -r .resolved.addresses.authorization)

# Activate the new authorization we just created
# (or deactivate it by flipping the flag back to false)
solana-toolbox --rpc=devnet instruction \
    $PSYCHE_AUTHORIZER_ID authorization_grantor_update \
    --args=params.active:true \
    grantor:$GRANTOR_KEYPAIR \
    authorization:$AUTHORIZATION_PUBKEY \
    --execute
```

## Setting up delegates from the authorized user

Here is an example from the user's perspective to add delegates for its own master key

```sh
# Deployed authorizer
PSYCHE_AUTHORIZER_ID="PsyAUmhpmiUouWsnJdNGFSX8vZ6rWjXjgDPHsgqPGyw"
# We need to define the SCOPE of our authorization (what it's for)
PSYCHE_AUTH_SCOPE="utf8:CoordinatorJoinRun"

# We must know who granted the authorization somehow (replace here)
GRANTOR_PUBKEY=$(solana-keygen pubkey authority.json)
# We must have access to our user's master key
GRANTEE_KEYPAIR=user.json
GRANTEE_PUBKEY=$(solana-keygen pubkey user.json)

# Find the authorization PDA that should have already been created for us
AUTHORIZATION_CREATE_JSON=$(\
    solana-toolbox --rpc=devnet instruction \
    $PSYCHE_AUTHORIZER_ID authorization_create \
    grantor:$GRANTOR_PUBKEY \
    --args=params.grantee:$GRANTEE_PUBKEY \
    --args=params.scope:$PSYCHE_AUTH_SCOPE
)

# Read the authorization PDA from the create instruction JSON
AUTHORIZATION_PUBKEY=$(echo $AUTHORIZATION_CREATE_JSON | jq -r .resolved.addresses.authorization)

# We can then create add a new delegate keypair
solana-keygen new -o delegate.json --no-bip39-passphrase
DELEGATE_PUBKEY=$(solana-keygen pubkey delegate.json)

# Then we add the delegate key to our list of delegates
solana-toolbox --rpc=devnet instruction \
    $PSYCHE_AUTHORIZER_ID authorization_grantee_update \
    payer:keypair \
    authorization:$AUTHORIZATION_PUBKEY \
    grantee:$GRANTEE_KEYPAIR \
    --args=params.delegates_clear:false \
    --args=params.delegates_added:"[\"$DELEGATE_PUBKEY\"]" \
    --execute
```
