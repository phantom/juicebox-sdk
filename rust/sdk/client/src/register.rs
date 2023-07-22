use rand::rngs::OsRng;
use tracing::instrument;

use juicebox_sdk_core::{
    oprf::{OprfKey, OprfResult},
    requests::{
        Register1Response, Register2Request, Register2Response, SecretsRequest, SecretsResponse,
    },
    types::{
        EncryptedUserSecretCommitment, RegistrationVersion, UnlockKeyTag,
        UserSecretEncryptionKeyScalarShare,
    },
};
use juicebox_sdk_secret_sharing::create_shares;

use crate::{
    auth, http,
    request::{join_at_least_threshold, RequestError},
    types::{
        derive_unlock_key_and_commitment, UserSecretEncryptionKey, UserSecretEncryptionKeyScalar,
    },
    Client, Pin, Policy, Realm, Sleeper, UserInfo, UserSecret,
};

/// Error return type for [`Client::register`].
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Copy)]
pub enum RegisterError {
    /// A realm rejected the `Client`'s auth token.
    InvalidAuth,

    /// A software error has occurred. This request should not be retried
    /// with the same parameters. Verify your inputs, check for software
    /// updates and try again.
    Assertion,

    /// A transient error in sending or receiving requests to a realm.
    /// This request may succeed by trying again with the same parameters.
    Transient,
}

impl<S: Sleeper, Http: http::Client, Atm: auth::AuthTokenManager> Client<S, Http, Atm> {
    pub(crate) async fn perform_register(
        &self,
        pin: &Pin,
        secret: &UserSecret,
        info: &UserInfo,
        policy: Policy,
    ) -> Result<(), RegisterError> {
        let register1_requests = self
            .configuration
            .realms
            .iter()
            .map(|realm| self.register1_on_realm(realm));
        join_at_least_threshold(register1_requests, self.configuration.register_threshold).await?;

        let version = RegistrationVersion::new_random(&mut OsRng);

        let (access_key, encryption_key_seed) = pin
            .hash(self.configuration.pin_hashing_mode, &version, info)
            .expect("pin hashing failed");

        let oprf_root_key = OprfKey::new_random(&mut OsRng);
        let oprf_key_shares: Vec<OprfKey> = create_shares(
            oprf_root_key.as_scalar(),
            self.configuration.recover_threshold,
            self.configuration.share_count(),
            &mut OsRng,
        )
        .map(|share| OprfKey::from(share.secret))
        .collect();

        let oprf_result = OprfResult::evaluate(&oprf_root_key, access_key.expose_secret());

        let (unlock_key, unlock_key_commitment) = derive_unlock_key_and_commitment(&oprf_result);

        let encryption_key_scalar = UserSecretEncryptionKeyScalar::new_random();
        let encryption_key_scalar_shares: Vec<UserSecretEncryptionKeyScalarShare> = create_shares(
            encryption_key_scalar.expose_secret(),
            self.configuration.recover_threshold,
            self.configuration.share_count(),
            &mut OsRng,
        )
        .map(|share| UserSecretEncryptionKeyScalarShare::from(share.secret))
        .collect();

        let encryption_key =
            UserSecretEncryptionKey::derive(&encryption_key_seed, &encryption_key_scalar);
        let encrypted_secret = secret.encrypt(&encryption_key);

        let register2_requests = zip3(
            &self.configuration.realms,
            oprf_key_shares,
            encryption_key_scalar_shares,
        )
        .map(|(realm, oprf_key_share, encryption_key_scalar_share)| {
            self.register2_on_realm(
                realm,
                Register2Request {
                    version: version.to_owned(),
                    oprf_key: oprf_key_share.to_owned(),
                    unlock_key_commitment: unlock_key_commitment.to_owned(),
                    unlock_key_tag: UnlockKeyTag::derive(&unlock_key, &realm.id),
                    user_secret_encryption_key_scalar_share: encryption_key_scalar_share.to_owned(),
                    encrypted_user_secret: encrypted_secret.to_owned(),
                    encrypted_user_secret_commitment: EncryptedUserSecretCommitment::derive(
                        &unlock_key,
                        &realm.id,
                        &encryption_key_scalar_share,
                        &encrypted_secret,
                    ),
                    policy: policy.to_owned(),
                },
            )
        });

        join_at_least_threshold(register2_requests, self.configuration.register_threshold).await?;

        Ok(())
    }

    /// Executes phase 1 of registration on a particular realm.
    #[instrument(level = "trace", skip(self), err(level = "trace", Debug))]
    async fn register1_on_realm(&self, realm: &Realm) -> Result<(), RegisterError> {
        match self.make_request(realm, SecretsRequest::Register1).await {
            Err(RequestError::InvalidAuth) => Err(RegisterError::InvalidAuth),
            Err(RequestError::Assertion) => Err(RegisterError::Assertion),
            Err(RequestError::Transient) => Err(RegisterError::Transient),
            Ok(SecretsResponse::Register1(Register1Response::Ok)) => Ok(()),
            Ok(_) => Err(RegisterError::Assertion),
        }
    }

    /// Executes phase 2 of registration on a particular realm.
    #[instrument(level = "trace", skip(self), err(level = "trace", Debug))]
    async fn register2_on_realm(
        &self,
        realm: &Realm,
        request: Register2Request,
    ) -> Result<(), RegisterError> {
        match self
            .make_request(realm, SecretsRequest::Register2(Box::new(request)))
            .await
        {
            Err(RequestError::InvalidAuth) => Err(RegisterError::InvalidAuth),
            Err(RequestError::Assertion) => Err(RegisterError::Assertion),
            Err(RequestError::Transient) => Err(RegisterError::Transient),
            Ok(SecretsResponse::Register2(Register2Response::Ok)) => Ok(()),
            Ok(_) => Err(RegisterError::Assertion),
        }
    }
}

fn zip3<A, B, C>(a: A, b: B, c: C) -> impl Iterator<Item = (A::Item, B::Item, C::Item)>
where
    A: IntoIterator,
    B: IntoIterator,
    C: IntoIterator,
{
    let iter = a.into_iter().zip(b).zip(c);
    iter.map(|((a, b), c)| (a, b, c))
}

mod tests {
    #[test]
    fn test_zip3() {
        let a = vec![1, 2, 3];
        let b = vec!['a', 'b', 'c'];
        let c = vec![true, false, true];

        let zipped: Vec<_> = super::zip3(a, b, c).collect();

        let expected = vec![(1, 'a', true), (2, 'b', false), (3, 'c', true)];

        assert_eq!(zipped, expected);
    }
}
