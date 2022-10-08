use crate::*;
use near_contract_standards::non_fungible_token::events::{NftMint, NftTransfer};
use near_sdk::require;

/// Ensures the attached_deposit is one yoctoNear
pub(crate) fn assert_one_yocto() {
    assert_eq!(
        env::attached_deposit(),
        1,
        " Requires attached deposit of exactly 1 yoctoNEAR"
    )
}
/// Ensures the attached_deposit is at least one yoctoNear
pub(crate) fn assert_at_least_one_yocto() {
    assert!(
        env::attached_deposit() >= 1,
        "Requires attached deposit of at least 1 yoctoNEAR",
    )
}

/// convert the royalty percentage and amount to pay into a payout (U128)
/// we use 100% to be 10,000 so that we can have percentages of less then 1
pub(crate) fn royalty_to_payout(royalty_percentage: u32, amount_to_pay: Balance) -> U128 {
    U128(royalty_percentage as u128 * amount_to_pay / 10_000u128)
}

impl Contract {
    /// Internal function to handle assemblying and updating the contract with the new NFT
    pub(crate) fn internal_mint(
        &mut self,
        token_owner_id: AccountId,
        token_id: TokenId,
        perpetual_royalties: Option<HashMap<AccountId, u32>>,
        token_metadata: TokenMetadata,
    ) -> TokenJson {
        // set royalty for contract owner on every mint
        let mut royalty = HashMap::from([(self.owner_id.clone(), 150)]);

        let mut total_perpetual = 150u32;
        // if we have perpetual royaties
        if let Some(perpetual_royalties) = perpetual_royalties {
            // check bleow gas limit of 5 royalties
            assert!(
                perpetual_royalties.len() <= 7,
                " Cannot add more than 7 perpetual royalties"
            );
            // loop through and add acounts t the list
            for (account, amount) in perpetual_royalties {
                assert!(
                    amount < MINTER_ROYALTY_CAP,
                    " The threshold royalty is capped at 70% for an accountId"
                );
                royalty.insert(account, amount);

                total_perpetual += amount;
                assert!(
                    total_perpetual <= MINTER_ROYALTY_CAP,
                    " The sum of all the perpetual royalties cannot be more than 70%"
                );
            }
        }
        // royalty limit for minter capped at 20%
        let token_royalty = &mut TokenRoyalty {
            royalty: royalty.clone(),
        };

        self.owner_by_id.insert(&token_id, &token_owner_id);
        self.token_metadata_by_id
            .as_mut()
            .and_then(|by_id| by_id.insert(&token_id, &token_metadata));
        self.royalty_by_id
            .as_mut()
            .and_then(|by_id| by_id.insert(&token_id, token_royalty));
        if let Some(tokens_per_owner) = &mut self.tokens_per_owner {
            let mut token_ids = tokens_per_owner.get(&token_owner_id).unwrap_or_else(|| {
                UnorderedSet::new(StorageKey::TokensPerOwnerInner {
                    account_hash: env::sha256(token_owner_id.as_bytes()),
                })
            });

            token_ids.insert(&token_id);
            tokens_per_owner.insert(&token_owner_id, &token_ids);
        }
        let approved_account_ids = if let Some(approvals_by_id) = &mut self.approvals_by_id {
            let approved_account_ids: HashMap<AccountId, u64> = HashMap::new();
            approvals_by_id.insert(&token_id, &approved_account_ids);
            Some(approved_account_ids)
        } else {
            None
        };
        if let Some(next_approval_id_by_id) = &mut self.next_approval_id_by_id {
            next_approval_id_by_id.insert(&token_id, &1u64);
        }

        NftMint {
            owner_id: &token_owner_id,
            token_ids: &[&token_id],
            memo: None,
        }
        .emit();

        TokenJson {
            token_id,
            owner_id: token_owner_id,
            metadata: Some(token_metadata),
            royalty: Some(royalty),
            approved_account_ids,
        }
    }

    /// Transfer token_id from `sender_id` to `receiver_id`
    ///
    /// Performs safety checks and logging
    pub(crate) fn internal_transfer(
        &mut self,
        sender_id: &AccountId,
        receiver_id: &AccountId,
        token_id: &TokenId,
        approval_id: Option<u64>,
        memo: &Option<String>,
    ) -> (AccountId, Option<HashMap<AccountId, u64>>) {
        let owner_id = self
            .owner_by_id
            .get(token_id)
            .expect(" Token doesn't exists!");

        let approved_account_ids = self
            .approvals_by_id
            .as_mut()
            .and_then(|by_id| by_id.remove(token_id));

        let sender_id = if sender_id != &owner_id {
            let app_acc_ids = approved_account_ids
                .as_ref()
                .unwrap_or_else(|| env::panic_str(" Unauthorized"));
            let actual_approval_id = app_acc_ids.get(sender_id);
            if actual_approval_id.is_none() {
                env::panic_str(" Sender not approved")
            }

            require!(
                approval_id.is_none() || actual_approval_id == approval_id.as_ref(),
                format!(
                    "The actual approval_id {:?} is different from the given approval_id {:?}",
                    actual_approval_id, approval_id
                )
            );
            Some(sender_id)
        } else {
            None
        };

        require!(
            &owner_id != receiver_id,
            " Current and next owner must differ"
        );
        self.internal_transfer_unguarded(token_id, &owner_id, receiver_id);

        NftTransfer {
            old_owner_id: &owner_id,
            new_owner_id: receiver_id,
            token_ids: &[token_id],
            authorized_id: sender_id.filter(|sender_id| *sender_id == &owner_id),
            memo: memo.as_deref(),
        }
        .emit();

        //return the preivous token object that was transferred.
        (owner_id, approved_account_ids)
    }

    /// Transfer token_id from `from` to `to`
    ///
    /// Do not perform any safety checks or do any logging
    pub(crate) fn internal_transfer_unguarded(
        &mut self,
        #[allow(clippy::ptr_arg)] token_id: &TokenId,
        from: &AccountId,
        to: &AccountId,
    ) {
        // update owner
        self.owner_by_id.insert(token_id, to);

        if let Some(tokens_per_owner) = &mut self.tokens_per_owner {
            let mut owner_tokens = tokens_per_owner.get(from).unwrap_or_else(|| {
                env::panic_str(" Unable to access tokens per owner in unguarded call.");
            });
            owner_tokens.remove(token_id);
            if owner_tokens.is_empty() {
                tokens_per_owner.remove(from);
            } else {
                tokens_per_owner.insert(from, &owner_tokens);
            }

            let mut receiver_tokens = tokens_per_owner.get(to).unwrap_or_else(|| {
                UnorderedSet::new(StorageKey::TokensPerOwnerInner {
                    account_hash: env::sha256(to.as_bytes()),
                })
            });
            receiver_tokens.insert(token_id);
            tokens_per_owner.insert(to, &receiver_tokens);
        }
    }
}
