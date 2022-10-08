use crate::*;
use near_contract_standards::non_fungible_token::events::NftTransfer;
use near_contract_standards::non_fungible_token::Token;
use std::collections::HashMap;

const GAS_FOR_RESOLVE_TRANSFER: Gas = Gas(5_000_000_000_000);
const GAS_FOR_NFT_TRANSFER_CALL: Gas = Gas(25_000_000_000_000 + GAS_FOR_RESOLVE_TRANSFER.0);

/// Used for all non-fungible tokens. The specification for the
/// [core non-fungible token standard] lays out the reasoning for each method.
/// It's important to check out [NonFungibleTokenReceiver](crate::NonFungibleTokenReceiver)
/// and [NonFungibleTokenResolver](crate::NonFungibleTokenResolver) to
/// understand how the cross-contract call work.
///
/// [core non-fungible token standard]: <https://nomicon.io/Standards/NonFungibleToken/Core.html>
pub trait NonFungibleTokenCore {
    /// Transfer Token from previous owner to receiver_id
    fn nft_transfer(
        &mut self,
        receiver_id: AccountId,
        token_id: TokenId,
        approval_id: Option<u64>,
        memo: Option<String>,
    );

    /// Transfer Token from previous owner to receiver_id and make a CCC on the receiver's account
    fn nft_transfer_call(
        &mut self,
        receiver_id: AccountId,
        token_id: TokenId,
        approval_id: Option<u64>,
        memo: Option<String>,
        msg: String,
    ) -> PromiseOrValue<bool>;

    /// Returns the Token with the given `token_id` or `None` if no such token.
    fn nft_token(&self, token_id: TokenId) -> Option<Token>;
}

/// Used when an NFT is transferred using `nft_transfer_call`. This trait is implemented on the receiving contract, not on the NFT contract.
#[ext_contract(ext_nft_receiver)]
pub trait NonFungibleTokenReceiver {
    fn nft_on_transfer(
        &mut self,
        sender_id: AccountId,
        previous_owner_id: AccountId,
        token_id: TokenId,
        msg: String,
    ) -> PromiseOrValue<bool>;
}

/// Used when an NFT is transferred using `nft_transfer_call`. This is the method that's called after `nft_on_transfer`. This trait is implemented on the NFT contract.
#[ext_contract(ext_nft_resolver)]
pub trait NonFungibleTokenResolver {
    fn nft_resolve_transfer(
        &mut self,
        previous_owner_id: AccountId,
        receiver_id: AccountId,
        token_id: TokenId,
        approvals: Option<HashMap<AccountId, u64>>,
    ) -> bool;
}

/// Used when an NFT is burnt using `remove_sale_and_burn` on the marketplace contract. This trait is implemented on the NFT contract.
pub trait NonFungibleTokenRemoveSaleAndBurn {
    // Burn an nft through an authorised marketplace
    fn nft_on_remove_sale_and_burn(
        &mut self,
        sender_id: AccountId,
        token_id: TokenId,
        approval_id: u64,
    );
}

#[near_bindgen]
impl NonFungibleTokenCore for Contract {
    #[payable]
    fn nft_transfer(
        &mut self,
        receiver_id: AccountId,
        token_id: TokenId,
        approval_id: Option<u64>,
        memo: Option<String>,
    ) {
        assert_one_yocto();
        let sender_id = env::predecessor_account_id();

        self.internal_transfer(&sender_id, &receiver_id, &token_id, approval_id, &memo);
    }

    #[payable]
    fn nft_transfer_call(
        &mut self,
        receiver_id: AccountId,
        token_id: TokenId,
        approval_id: Option<u64>,
        memo: Option<String>,
        msg: String,
    ) -> PromiseOrValue<bool> {
        assert_one_yocto();
        require!(
            env::prepaid_gas() > GAS_FOR_NFT_TRANSFER_CALL,
            "cypher: More gas is required"
        );
        let sender_id = env::predecessor_account_id();
        let (old_owner, old_approvals) =
            self.internal_transfer(&sender_id, &receiver_id, &token_id, approval_id, &memo);

        // Initiating receiver's call and the callback
        ext_nft_receiver::ext(receiver_id.clone())
            .with_static_gas(env::prepaid_gas() - GAS_FOR_NFT_TRANSFER_CALL)
            .nft_on_transfer(sender_id, old_owner.clone(), token_id.clone(), msg)
            .then(
                ext_nft_resolver::ext(env::current_account_id())
                    .with_static_gas(GAS_FOR_RESOLVE_TRANSFER)
                    .nft_resolve_transfer(old_owner, receiver_id, token_id, old_approvals),
            )
            .into()
    }

    fn nft_token(&self, token_id: TokenId) -> Option<Token> {
        let owner_id = self.owner_by_id.get(&token_id)?;
        let metadata =  {
            self.token_metadata_by_id
                .as_ref()
                .and_then(|by_id| by_id.get(&token_id))
        };
        let approved_account_ids = self
            .approvals_by_id
            .as_ref()
            .and_then(|by_id| by_id.get(&token_id).or_else(|| Some(HashMap::new())));
        Some(Token {
            token_id,
            owner_id,
            metadata,
            approved_account_ids,
        })
    }
}

#[near_bindgen]
impl NonFungibleTokenResolver for Contract {
    /// Returns true if token was successfully transferred to `receiver_id`.
    fn nft_resolve_transfer(
        &mut self,
        previous_owner_id: AccountId,
        receiver_id: AccountId,
        token_id: TokenId,
        approved_account_ids: Option<HashMap<AccountId, u64>>,
    ) -> bool {
        // Get whether token should be returned
        let must_revert = match env::promise_result(0) {
            PromiseResult::NotReady => env::abort(),
            PromiseResult::Successful(value) => {
                if let Ok(yes_or_no) = near_sdk::serde_json::from_slice::<bool>(&value) {
                    yes_or_no
                } else {
                    true
                }
            }
            PromiseResult::Failed => true,
        };

        // if call succeeded, return early
        if !must_revert {
            return true;
        }

        // OTHERWISE, try to set owner back to previous_owner_id and restore approved_account_ids

        // Check that receiver didn't already transfer it away or burn it.
        if let Some(current_owner) = self.owner_by_id.get(&token_id) {
            if current_owner != receiver_id {
                // The token is not owned by the receiver anymore. Can't return it.
                return true;
            }
        } else {
            // The token was burned and doesn't exist anymore.
            // Refund storage cost for storing approvals to original owner and return early.
            if let Some(approved_account_ids) = approved_account_ids {
                refund_approved_account_ids(previous_owner_id, &approved_account_ids);
            }
            return true;
        };

        self.internal_transfer_unguarded(&token_id, &receiver_id, &previous_owner_id);

        // If using Approval Management extension,
        // 1. revert any approvals receiver already set, refunding storage costs
        // 2. reset approvals to what previous owner had set before call to nft_transfer_call
        if let Some(by_id) = &mut self.approvals_by_id {
            if let Some(receiver_approvals) = by_id.get(&token_id) {
                refund_approved_account_ids(receiver_id.clone(), &receiver_approvals);
            }
            if let Some(previous_owner_approvals) = approved_account_ids {
                by_id.insert(&token_id, &previous_owner_approvals);
            }
        }
        NftTransfer {
            old_owner_id: &receiver_id,
            new_owner_id: &previous_owner_id,
            token_ids: &[&token_id],
            authorized_id: None,
            memo: None,
        }
        .emit();
        false
    }
}