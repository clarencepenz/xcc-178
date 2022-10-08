use crate::*;

#[near_bindgen]
impl Contract {
    /// get the total supply of NFTs for the contract
    pub fn nft_total_supply(&self) -> U128 {
        (self.owner_by_id.len() as u128).into()
    }

    /// Query for nft tokens on the contract regardless of the owner using pagination
    pub fn nft_tokens(&self, from_index: Option<U128>, limit: Option<u64>) -> Vec<TokenJson> {
        let start_index: u128 = from_index.map(From::from).unwrap_or_default();
        require!(
            (self.owner_by_id.len() as u128) >= start_index,
            "cypher: Out of bounds, please use a smaller from_index."
        );
        let limit = limit.map(|v| v as usize).unwrap_or(usize::MAX);
        require!(limit != 0, "cypher: Cannot provide limit of 0.");

        self.owner_by_id
            .iter_rev()
            .skip(start_index as usize)
            .take(limit)
            .map(|(token_id, _)| self.enum_nft_token(token_id))
            .collect()
    }

    /// get the total supply of NFTs for a given owner
    pub fn nft_supply_for_owner(&self, account_id: AccountId) -> U128 {
        let tokens_per_owner = self.tokens_per_owner.as_ref().unwrap_or_else(|| {
            env::panic_str(
                "cypher: Could not find tokens_per_owner when calling a method on the \
                enumeration standard.",
            )
        });
        tokens_per_owner
            .get(&account_id)
            .map(|account_tokens| U128::from(account_tokens.len() as u128))
            .unwrap_or(U128(0))
    }

    /// Query for all the tokens for an owner
    pub fn nft_tokens_for_owner(
        &self,
        account_id: AccountId,
        from_index: Option<U128>,
        limit: Option<u64>,
    ) -> Vec<TokenJson> {
        let tokens_per_owner = self.tokens_per_owner.as_ref().unwrap_or_else(|| {
            env::panic_str(
                "cypher: Could not find tokens_per_owner when calling a method on the \
                enumeration standard.",
            )
        });
        let token_set = if let Some(token_set) = tokens_per_owner.get(&account_id) {
            token_set
        } else {
            return vec![];
        };
        let limit = limit.map(|v| v as usize).unwrap_or(usize::MAX);
        require!(limit != 0, "cypher: Cannot provide limit of 0.");
        let start_index: u128 = from_index.map(From::from).unwrap_or_default();
        require!(
            token_set.len() as u128 > start_index,
            "cypher: Out of bounds, please use a smaller from_index."
        );
        token_set
            .iter()
            .skip(start_index as usize)
            .take(limit)
            .map(|token_id| self.enum_nft_token(token_id))
            .collect()
    }

    /// get the information for a specific token ID
    pub fn enum_nft_token(&self, token_id: TokenId) -> TokenJson {
        let owner_id = self.owner_by_id.get(&token_id).unwrap_or_else(|| {
            env::panic_str("cypher: Token doesn't exist");
        });
        let metadata =  {
            self.token_metadata_by_id
                .as_ref()
                .and_then(|by_id| by_id.get(&token_id))
        };
        let approved_account_ids = self
            .approvals_by_id
            .as_ref()
            .and_then(|by_id| by_id.get(&token_id).or_else(|| Some(HashMap::new())));
        let royalty = if let Some(royalty_by_id) = &self.royalty_by_id {
            let token_royalty = royalty_by_id.get(&token_id).unwrap();
            Some(token_royalty.royalty)
        } else {
            None
        };
        TokenJson {
            token_id,
            owner_id,
            metadata,
            royalty,
            approved_account_ids,
        }
    }
}
