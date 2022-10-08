use std::collections::HashMap;
use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_contract_standards::non_fungible_token::metadata::{
    NFTContractMetadata, NonFungibleTokenMetadataProvider, TokenMetadata,
};
use near_sdk::collections::{LazyOption, LookupMap, LookupSet, TreeMap, UnorderedSet};
use near_sdk::json_types::U128;
use near_sdk::serde::{Deserialize, Serialize};
use near_sdk::{
    env, ext_contract, near_bindgen, require, AccountId, Balance,
    BorshStorageKey, Gas, IntoStorageKey, PanicOnDefault, PromiseOrValue, PromiseResult,
};

use crate::internal::*;
pub use crate::metadata::*;
pub use crate::mint::*;
pub use crate::nft_core::*;
pub use crate::approval::*;
pub use crate::royalty::*;
pub use crate::events::*;
pub use crate::utils::*;

mod internal;
mod approval; 
mod enumeration; 
mod metadata; 
mod mint; 
mod nft_core; 
mod royalty; 
mod events;
mod utils;

/// This spec can be treated like a version of the standard.
pub const NFT_METADATA_SPEC: &str = "1.0.0";
/// This is the name of the NFT standard we're using
pub const NFT_STANDARD_NAME: &str = "nep178";
// Total royalty on a particular NFT
pub const MINTER_ROYALTY_CAP: u32 = 7000;
#[near_bindgen]
#[derive(BorshDeserialize, BorshSerialize, PanicOnDefault)]
pub struct Contract {
    //contract owner
    pub owner_id: AccountId,

    //keeps track of all the token IDs for a given account
    pub tokens_per_owner: Option<LookupMap<AccountId, UnorderedSet<TokenId>>>,

    //keeps track of the token struct for a given token ID
    pub owner_by_id: TreeMap<TokenId, AccountId>,

    //keeps track of the token metadata for a given token ID
    pub token_metadata_by_id: Option<LookupMap<TokenId, TokenMetadata>>,

    //keeps track of the metadata for the contract
    pub metadata: LazyOption<NFTContractMetadata>,

    // Approval managemeent
    pub approvals_by_id: Option<LookupMap<TokenId, HashMap<AccountId, u64>>>,
    pub next_approval_id_by_id: Option<LookupMap<TokenId, u64>>,

    // Royalty
    pub royalty_by_id: Option<LookupMap<TokenId, TokenRoyalty>>,

     //keep track of accounts that can mint NFTs
     pub allow_list: LookupSet<AccountId>,
}

/// Helper structure for keys of the persistent collections.
#[derive(BorshStorageKey, BorshSerialize)]
pub enum StorageKey {
    TokensPerOwner,
    TokensPerOwnerInner { account_hash: Vec<u8> },
    TokenMetadataById,
    NFTContractMetadata,
    TokenTypesLocked,
    ApprovalPrefix,
    TokenById,
    AllowList
}

#[near_bindgen]
impl Contract {
    /*
        initialization function (can only be called once).
        this initializes the contract with default metadata so the
        user doesn't have to manually type metadata.
    */
    #[init]
    pub fn new_default_meta(owner_id: AccountId) -> Self {
        //calls the other function "new: with some default metadata and the owner_id passed in 
        Self::new(
            owner_id,
            NFTContractMetadata {
                spec: "nft-1.0.0".to_string(),
                name: "Captain NFT".to_string(),
                symbol: "CPT".to_string(),
                icon: None,
                base_uri: None,
                reference: None,
                reference_hash: None,
            },
        )
    }

    /*
        initialization function (can only be called once).
        this initializes the contract with metadata that was passed in and
        the owner_id. 
    */
    #[init]
    pub fn new(owner_id: AccountId, metadata: NFTContractMetadata) -> Self {
        let (approvals_by_id, next_approval_id_by_id) = {
            let prefix = StorageKey::ApprovalPrefix.into_storage_key();
            (
                Some(LookupMap::new(prefix.clone())),
                Some(LookupMap::new([prefix, "n".into()].concat())),
            )
        };
        //create a variable of type Self with all the fields initialized. 
        Self {
            //Storage keys are simply the prefixes used for the collections. This helps avoid data collision
            tokens_per_owner: Some(LookupMap::new(
                StorageKey::TokensPerOwner.into_storage_key(),
            )),
            owner_by_id: TreeMap::new(StorageKey::TokenById.try_to_vec().unwrap()),
            token_metadata_by_id: Some(LookupMap::new(
                StorageKey::TokenMetadataById.into_storage_key(),
            )),
            //set the owner_id field equal to the passed in owner_id. 
            owner_id,
            approvals_by_id,
            royalty_by_id: Some(LookupMap::new(StorageKey::TokenById.into_storage_key())),
            next_approval_id_by_id,
            metadata: LazyOption::new(
                StorageKey::NFTContractMetadata.into_storage_key(),
                Some(&metadata),
            ),
            allow_list: LookupSet::new(StorageKey::AllowList.try_to_vec().unwrap())
        }
    }

    pub fn allow_minting_access(&mut self, account_id: AccountId) {
        assert_eq!(
            env::predecessor_account_id(),
            self.owner_id,
            "Only owner can allow minting access",
        );

        self.allow_list.insert(&account_id);
    }

    pub fn revoke_minting_access(&mut self, account_id: AccountId) {
        assert_eq!(
            env::predecessor_account_id(),
            self.owner_id,
            "Only owner can revoke minting access",
        );

        self.allow_list.remove(&account_id);
    }
}