use crate::*;

pub trait NonFungibleTokenRoyalty {
    /// calculates the payout for a token given the passed in balance. This is a view method
    fn nft_payout(&self, token_id: String, balance: U128, max_len_payout: u32) -> Payout;

    /// transfers the token to the receiver ID and returns the payout object that should be payed given the passed in balance.
    fn nft_transfer_payout(
        &mut self,
        receiver_id: AccountId,
        token_id: TokenId,
        approval_id: Option<u64>,
        balance: Option<U128>,
        max_len_payout: Option<u32>,
    ) -> Option<Payout>;
}

#[near_bindgen]
impl NonFungibleTokenRoyalty for Contract {
    /// calculates the payout for a token given the passed in balance. This is a view method
    fn nft_payout(&self, token_id: String, balance: U128, max_len_payout: u32) -> Payout {
        let owner_id = self
            .owner_by_id
            .get(&token_id)
            .expect("cypher: Token doesn't exists!");
        let royalty = if let Some(royalty_by_id) = &self.royalty_by_id {
            royalty_by_id.get(&token_id).unwrap().royalty
        } else {
            HashMap::new()
        };

        assert!(
            royalty.len() as u32 <= max_len_payout,
            "cypher: Market cannot payout to that many receivers"
        );

        let balance_u128: u128 = balance.into();

        let mut payout: Payout = Payout {
            payout: HashMap::new(),
        };
        let mut total_perpetual = 0;

        for (k, v) in royalty.iter() {
            if *k != owner_id {
                let key = k.clone();
                payout
                    .payout
                    .insert(key, royalty_to_payout(*v, balance_u128));
                total_perpetual += *v;
            }
        }
        payout.payout.insert(
            owner_id,
            royalty_to_payout(10000 - total_perpetual, balance_u128),
        );
        payout
    }

    /// transfers the token to the receiver ID and returns the payout object that should be payed given the passed in balance.
    #[payable]
    fn nft_transfer_payout(
        &mut self,
        receiver_id: AccountId,
        token_id: TokenId,
        approval_id: Option<u64>,
        balance: Option<U128>,
        max_len_payout: Option<u32>,
    ) -> Option<Payout> {
        assert_one_yocto();
        let sender_id = env::predecessor_account_id();
        let royalty = if let Some(royalty_by_id) = &self.royalty_by_id {
            royalty_by_id.get(&token_id).unwrap().royalty
        } else {
            HashMap::new()
        };

        //transfer the token to the passed in receiver and get the previous token object back
        let (previous_owner_id, _) =
            self.internal_transfer(&sender_id, &receiver_id, &token_id, approval_id, &None);

        let mut total_perpetual = 0;
        let payout = if let Some(balance) = balance {
            let balance_u128: u128 = u128::from(balance);
            let mut payout: Payout = Payout {
                payout: HashMap::new(),
            };

            assert!(
                royalty.len() as u32 <= max_len_payout.unwrap(),
                "cypher: Market cannot pay that many people"
            );

            for (k, v) in royalty.iter() {
                let key = k.clone();
                if key != previous_owner_id {
                    payout
                        .payout
                        .insert(key, royalty_to_payout(*v, balance_u128));
                    total_perpetual += *v;
                }
            }
            assert!(
                total_perpetual <= MINTER_ROYALTY_CAP,
                "cypher: Royalties should not be more than caps of 70%"
            );
            payout.payout.insert(
                previous_owner_id,
                royalty_to_payout(10000 - total_perpetual, balance_u128),
            );
            Some(payout)
        } else {
            None
        };

        payout
    }
}