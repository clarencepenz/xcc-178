use crate::*;

#[near_bindgen]
impl Contract {
    #[payable]
    pub fn nft_mint(
        &mut self,
        token_id: TokenId,
        token_metadata: TokenMetadata,
        receiver_id: AccountId,
        perpetual_royalties: Option<HashMap<AccountId, u32>>,
    ) {
        if self.owner_by_id.get(&token_id).is_some() {
            env::panic_str("cypher: token_id must be unique");
        }
        // abstracts the minting procedure
        self.internal_mint(
            receiver_id,
            token_id,
            perpetual_royalties,
            token_metadata,
        );
    }
}