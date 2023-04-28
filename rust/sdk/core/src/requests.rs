extern crate alloc;

use alloc::vec::Vec;
use core::fmt;
use core::time::Duration;
use serde::{Deserialize, Serialize};

use crate::types::{
    AuthToken, GenerationNumber, MaskedTgkShare, OprfBlindedInput, OprfBlindedResult, OprfKey,
    Policy, RealmId, Salt, SessionId, UnlockTag, UserSecretShare,
};
use loam_sdk_noise as noise;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ClientRequest {
    pub realm: RealmId,
    pub auth_token: AuthToken,
    pub session_id: SessionId,
    pub kind: ClientRequestKind,
    pub encrypted: NoiseRequest,
}

/// Used in [`ClientRequest`].
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum ClientRequestKind {
    /// The [`ClientRequest`] contains just a Noise handshake request, without
    /// a [`SecretsRequest`]. The server does not need to access the user's
    /// record to process this.
    HandshakeOnly,
    /// The [`ClientRequest`] contains a Noise handshake or transport request
    /// with an encrypted [`SecretsRequest`]. The server will need to access
    /// the user's record to process this.
    SecretsRequest,
}

#[derive(Debug, Deserialize, Serialize)]
#[allow(clippy::large_enum_variant)]
pub enum ClientResponse {
    Ok(NoiseResponse),
    /// The appropriate server to handle the request is not currently
    /// available.
    Unavailable,
    /// The request's auth token is not acceptable.
    InvalidAuth,
    /// The server could not find the Noise session state referenced by the
    /// request's session ID. This can occur in normal circumstances when a
    /// server restarts or has expired the session. The client should open a
    /// new session.
    MissingSession,
    /// The server could not decrypt the encapsulated Noise request.
    SessionError,
    // The server could not deserialize the [`ClientRequest`] or the
    // encapsulated [`SecretsRequest`].
    DecodingError,
}

#[derive(Clone, Deserialize, Serialize)]
pub enum NoiseRequest {
    Handshake { handshake: noise::HandshakeRequest },
    Transport { ciphertext: Vec<u8> },
}

impl fmt::Debug for NoiseRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Handshake { .. } => f
                .debug_struct("NoiseRequest::Handshake")
                .finish_non_exhaustive(),
            Self::Transport { .. } => f
                .debug_struct("NoiseRequest::Transport")
                .finish_non_exhaustive(),
        }
    }
}

#[derive(Deserialize, Serialize)]
pub enum NoiseResponse {
    Handshake {
        handshake: noise::HandshakeResponse,
        /// Once the session becomes inactive for this many milliseconds, the
        /// client should discard the session.
        session_lifetime: Duration,
    },
    Transport {
        ciphertext: Vec<u8>,
    },
}

impl fmt::Debug for NoiseResponse {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Handshake {
                session_lifetime, ..
            } => f
                .debug_struct("NoiseResponse::Handshake")
                .field("session_lifetime", &session_lifetime)
                .finish_non_exhaustive(),
            Self::Transport { .. } => f
                .debug_struct("NoiseResponse::Transport")
                .finish_non_exhaustive(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub enum SecretsRequest {
    Register1,
    Register2(Register2Request),
    Recover1(Recover1Request),
    Recover2(Recover2Request),
    Delete(DeleteRequest),
}

impl SecretsRequest {
    /// Returns whether the request type requires forward secrecy.
    ///
    /// This controls whether the request may be sent as part of a Noise NK
    /// handshake request, which does not provide forward secrecy.
    ///
    /// For more sensitive request types, this returns true, requiring an
    /// established Noise session before the request can be sent. Decrypting
    /// these requests would require both the server/realm's static secret key
    /// and the ephemeral key used only for this session.
    ///
    /// For less sensitive request types, this returns false, indicating that
    /// they can be piggy-backed with the Noise NK handshake request.
    /// Decrypting these requests would be possible with just the
    /// server/realm's static secret key (even any time in the future).
    pub fn needs_forward_secrecy(&self) -> bool {
        match self {
            Self::Register1 => false,
            Self::Register2(_) => true,
            Self::Recover1(_) => false,
            Self::Recover2(_) => true,
            Self::Delete(_) => false,
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[allow(clippy::large_enum_variant)]
pub enum SecretsResponse {
    Register1(Register1Response),
    Register2(Register2Response),
    Recover1(Recover1Response),
    Recover2(Recover2Response),
    Delete(DeleteResponse),
}

/// Response message for the first phase of registration.
#[derive(Debug, Deserialize, Serialize)]
pub struct Register1Response {
    pub next_generation_number: GenerationNumber,
}

/// Request message for the second phase of registration.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Register2Request {
    pub generation: GenerationNumber,
    pub salt: Salt,
    pub oprf_key: OprfKey,
    pub tag: UnlockTag,
    pub masked_tgk_share: MaskedTgkShare,
    pub secret_share: UserSecretShare,
    pub policy: Policy,
}

/// Response message for the second phase of registration.
#[derive(Debug, Deserialize, Serialize)]
pub enum Register2Response {
    Ok,
    AlreadyRegistered,
    BadGeneration,
}

/// Request message for the first phase of recovery.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Recover1Request {
    /// Which generation to recover. If the generation number is not provided, the
    /// server will start recovery with the latest generation.
    pub generation: Option<GenerationNumber>,
    pub blinded_pin: OprfBlindedInput,
}

/// Response message for the first phase of recovery.
#[derive(Debug, Deserialize, Serialize)]
pub enum Recover1Response {
    Ok {
        generation: GenerationNumber,
        blinded_oprf_pin: OprfBlindedResult,
        masked_tgk_share: MaskedTgkShare,
        /// The largest-numbered generation record on the server that's older
        /// than `generation`, if any. This allows the client to discover older
        /// generations to clean up or try recovering.
        previous_generation: Option<GenerationNumber>,
    },
    NotRegistered {
        generation: Option<GenerationNumber>,
        previous_generation: Option<GenerationNumber>,
    },
    PartiallyRegistered {
        generation: GenerationNumber,
        previous_generation: Option<GenerationNumber>,
    },
    NoGuesses {
        generation: GenerationNumber,
        previous_generation: Option<GenerationNumber>,
    },
}

/// Request message for the second phase of recovery.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Recover2Request {
    pub generation: GenerationNumber,
    pub tag: UnlockTag,
}

/// Response message for the second phase of recovery.
#[derive(Debug, Deserialize, Serialize)]
pub enum Recover2Response {
    Ok(UserSecretShare),
    NotRegistered,
    BadUnlockTag { guesses_remaining: u16 },
}

/// Request message to delete registered secrets.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct DeleteRequest {
    /// If `Some`, the server deletes generations from 0 up to and excluding
    /// this number. If `None`, the server deletes all generations.
    pub up_to: Option<GenerationNumber>,
}

/// Response message to delete registered secrets.
#[derive(Debug, Deserialize, Serialize)]
pub enum DeleteResponse {
    Ok,
}
