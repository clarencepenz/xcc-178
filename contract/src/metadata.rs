use crate::*;
pub type TokenId = String;
//defines the payout type we'll be returning as a part of the royalty standards.
#[derive(Serialize, Deserialize)]
#[serde(crate = "near_sdk::serde")]
pub struct Payout {
    pub payout: HashMap<AccountId, U128>,
}

///defines the royalty type we'll be using to store royalties for a TokenId
#[derive(BorshDeserialize, BorshSerialize)]
pub struct TokenRoyalty {
    pub royalty: HashMap<AccountId, u32>,
}

/// The token Json is what will be returned from view calls.
#[derive(Serialize, Deserialize)]
#[serde(crate = "near_sdk::serde")]
pub struct TokenJson {
    pub token_id: TokenId,
    pub owner_id: AccountId,
    pub metadata: Option<TokenMetadata>,
    pub royalty: Option<HashMap<AccountId, u32>>,
    pub approved_account_ids: Option<HashMap<AccountId, u64>>,
}

#[near_bindgen]
impl NonFungibleTokenMetadataProvider for Contract {
    /// Query for the basic contract information
    fn nft_metadata(&self) -> NFTContractMetadata {
        self.metadata.get().unwrap()
    }
}
