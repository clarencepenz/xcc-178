use crate::*;
use near_sdk::{env, ext_contract, require, AccountId, Gas, Promise};

const GAS_FOR_NFT_APPROVE: Gas = Gas(10_000_000_000_000);

pub trait NonFungibleTokenApproval {
    //approve an account ID to transfer a token on your behalf
    fn nft_approve(
        &mut self,
        token_id: TokenId,
        account_id: AccountId,
        msg: Option<String>,
    ) -> Option<Promise>;

    //check if the passed in account has access to approve the token ID
    fn nft_is_approved(
        &self,
        token_id: TokenId,
        approved_account_id: AccountId,
        approval_id: Option<u64>,
    ) -> bool;

    //revoke a specific account from transferring the token on your behalf
    fn nft_revoke(&mut self, token_id: TokenId, account_id: AccountId);

    //revoke all accounts from transferring the token on your behalf
    fn nft_revoke_all(&mut self, token_id: TokenId);
}

#[ext_contract(ext_non_fungible_approval_receiver)]
trait NonFungibleTokenApprovalsReceiver {
    /// XCC to an external contract that is initiated during nft_approve
    fn nft_on_approve(
        &mut self,
        token_id: TokenId,
        owner_id: AccountId,
        approval_id: u64,
        msg: String,
    );
}

/// Returns the Token if exists
fn expect_token_found<T>(option: Option<T>) -> T {
    option.unwrap_or_else(|| env::panic_str("cypher: Token not found"))
}

/// Returns the next_approval_by_id within Some(..)
fn expect_approval<T>(option: Option<T>) -> T {
    option.unwrap_or_else(|| {
        env::panic_str("cypher: next_approval_by_id must be set for approval ext")
    })
}

#[near_bindgen]
impl NonFungibleTokenApproval for Contract {
    /// allow a specific account ID to approve a token on your behalf
    #[payable]
    fn nft_approve(
        &mut self,
        token_id: TokenId,
        account_id: AccountId,
        msg: Option<String>,
    ) -> Option<Promise> {
        assert_at_least_one_yocto();

        // Ensure the contract implements the Approval management
        let approvals_by_id = self.approvals_by_id.as_mut().unwrap_or_else(|| {
            env::panic_str("cypher: NFT does not support Approval Management")
        });
        let owner_id = expect_token_found(self.owner_by_id.get(&token_id));

        require!(
            env::predecessor_account_id() == owner_id,
            "cypher: Predecessor must be token owner"
        );

        // update Hashmap of approvals for this token
        let next_approval_by_id = expect_approval(self.next_approval_id_by_id.as_mut());
        let approved_account_ids = &mut approvals_by_id.get(&token_id).unwrap_or_default();
        let approval_id = next_approval_by_id.get(&token_id).unwrap_or(1u64);
        let old_approval_id = approved_account_ids.insert(account_id.clone(), approval_id);
        approvals_by_id.insert(&token_id, approved_account_ids);
        next_approval_by_id.insert(&token_id, &(approval_id + 1));

        // calculate cost for storing new authorized AccountId
        let storage_used = if old_approval_id.is_none() {
            bytes_for_approved_account_id(&account_id)
        } else {
            0
        };
        refund_deposit(storage_used);

        // CCC to marketplace contract to list NFT for sale
        msg.map(|msg| {
            ext_nft_approval_receiver::ext(account_id)
                .with_static_gas(env::prepaid_gas() - GAS_FOR_NFT_APPROVE)
                .nft_on_approve(token_id, owner_id, approval_id, msg)
        })
    }

    /// check if the passed in account has access to approve the token ID
    fn nft_is_approved(
        &self,
        token_id: TokenId,
        approved_account_id: AccountId,
        approval_id: Option<u64>,
    ) -> bool {
        expect_token_found(self.owner_by_id.get(&token_id));
        let approvals_by_id = if let Some(a) = self.approvals_by_id.as_ref() {
            a
        } else {
            // contract does not support approval management
            return false;
        };

        let approved_account_ids = if let Some(ids) = approvals_by_id.get(&token_id) {
            ids
        } else {
            // token has no approvals
            return false;
        };

        let actual_approval_id = if let Some(id) = approved_account_ids.get(&approved_account_id) {
            id
        } else {
            // account not in approvals HashMap
            return false;
        };
        if let Some(given_approval_id) = approval_id {
            &given_approval_id == actual_approval_id
        } else {
            // account approved, no approval_id given
            true
        }
    }

    /// revoke a specific account from transferring the token on your behalf
    #[payable]
    fn nft_revoke(&mut self, token_id: TokenId, account_id: AccountId) {
        assert_one_yocto();

        // Ensure the contract implements the Approval management
        let approvals_by_id = self.approvals_by_id.as_mut().unwrap_or_else(|| {
            env::panic_str("cypher: NFT does not support Approval Management");
        });

        let owner_id = expect_token_found(self.owner_by_id.get(&token_id));
        let predecessor_account_id = env::predecessor_account_id();

        require!(
            predecessor_account_id == owner_id,
            "cypher: Predecessor must be token owner."
        );

        // if token has no approvals, do nothing
        if let Some(approved_account_ids) = &mut approvals_by_id.get(&token_id) {
            if approved_account_ids.remove(&account_id).is_some() {
                refund_approved_account_ids_iter(
                    predecessor_account_id,
                    core::iter::once(&account_id),
                );
                if approved_account_ids.is_empty() {
                    approvals_by_id.remove(&token_id);
                } else {
                    approvals_by_id.insert(&token_id, approved_account_ids);
                }
            }
        }
    }

    /// revoke all accounts from transferring the token on your behalf
    #[payable]
    fn nft_revoke_all(&mut self, token_id: TokenId) {
        assert_one_yocto();

        // Ensure the contract implements the Approval management
        let approvals_by_id = self.approvals_by_id.as_mut().unwrap_or_else(|| {
            env::panic_str("cypher: NFT does not support Approval Management");
        });

        let owner_id = expect_token_found(self.owner_by_id.get(&token_id));
        let predecessor_account_id = env::predecessor_account_id();

        require!(
            predecessor_account_id == owner_id,
            "cypher: Predecessor must be token owner."
        );

        // if token has no approvals, do nothing
        if let Some(approved_account_ids) = &mut approvals_by_id.get(&token_id) {
            // otherwise, refund owner for storage costs of all approvals...
            refund_approved_account_ids(predecessor_account_id, approved_account_ids);
            // ...and remove whole HashMap of approvals
            approvals_by_id.remove(&token_id);
        }
    }
}

/// Approval receiver is the trait for the method called (or attempted to be called) when an NFT contract adds an approval for an account.
#[ext_contract(ext_nft_approval_receiver)]
pub trait NonFungibleTokenApprovalReceiver {
    /// Respond to notification that contract has been granted approval for a token.
    ///
    /// Notes
    /// * Contract knows the token contract ID from `predecessor_account_id`
    ///
    /// Arguments:
    /// * `token_id`: the token to which this contract has been granted approval
    /// * `owner_id`: the owner of the token
    /// * `approval_id`: the approval ID stored by NFT contract for this approval.
    ///   Expected to be a number within the 2^53 limit representable by JSON.
    /// * `msg`: specifies information needed by the approved contract in order to
    ///    handle the approval. Can indicate both a function to call and the
    ///    parameters to pass to that function.
    fn nft_on_approve(
        &mut self,
        token_id: TokenId,
        owner_id: AccountId,
        approval_id: u64,
        msg: String,
    ) -> Option<near_sdk::PromiseOrValue<String>>; // TODO: how to make "any"?
}


